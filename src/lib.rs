pub(crate) mod d3d;

pub(crate) mod images;

pub mod asset;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use std::{
    cmp,
    error::Error,
    fmt::Display,
    io::{Cursor, Read, Seek, SeekFrom, Write},
    ops::Range,
};

use crate::asset::{
    ASSET_DESCRIPTION_SIZE, Asset, AssetDescription, AssetDescriptor, AssetError, AssetParseError,
    DataViewList, RawAsset,
};

pub mod game;

#[derive(Debug, Copy, Clone, Default)]
pub struct DataView {
    offset: u32,
    size: u32,
}

impl DataView {
    pub fn from_cursor<T>(cur: &mut Cursor<T>) -> Result<DataView, std::io::Error>
    where
        Cursor<T>: std::io::Read,
    {
        let offset = cur.read_u32::<LittleEndian>()?;
        let size = cur.read_u32::<LittleEndian>()?;

        Ok(DataView { offset, size })
    }

    pub fn as_range<T: From<u32>>(&self) -> Range<T> {
        let start: T = self.offset.into();
        let end: T = (self.offset + self.size).into();

        start..end
    }

    pub fn overlaps(&self, other: &DataView) -> bool {
        let r1: Range<u32> = self.as_range();
        let r2: Range<u32> = other.as_range();

        r1.start < r2.end && r2.start < r1.end

        /*
        let start1 = range.start;
        let end1 = range.end;

        let start2 = asset_desc.descriptor_ptr as usize;
        let end2 = start2 + asset_desc.descriptor_size as usize;

        start1 < end2 && start2 < end1
        */
    }
}

#[derive(Debug)]
pub enum BNLError {
    /// The ZLIB portion of the BNL file could not be decompressed successfully.
    DecompressionFailure,
    /// An error occurred when parsing the [`AssetDescription`] data of the BNL file.
    DataReadError(String),
}

impl From<std::io::Error> for BNLError {
    fn from(value: std::io::Error) -> Self {
        BNLError::DataReadError(format!("File error: {}", value))
    }
}

impl From<miniz_oxide::inflate::DecompressError> for BNLError {
    fn from(_: miniz_oxide::inflate::DecompressError) -> Self {
        BNLError::DecompressionFailure
    }
}

#[derive(Debug, Default)]
struct BNLHeader {
    file_count: u16,
    flags: u8,
    unknown_2: [u8; 5],

    asset_desc_loc: DataView,
    buffer_views_loc: DataView,
    buffer_loc: DataView,
    descriptor_loc: DataView,
}

impl BNLHeader {
    pub fn to_bytes(&self) -> [u8; 40] {
        let mut bytes = [0x00; 40];

        let mut cur = Cursor::new(&mut bytes[..]);

        cur.write_u16::<LittleEndian>(self.file_count).unwrap();
        cur.write_u8(self.flags).unwrap();

        self.unknown_2.iter().for_each(|val| {
            cur.write_u8(*val).unwrap();
        });

        cur.write_u32::<LittleEndian>(self.asset_desc_loc.offset)
            .unwrap();
        cur.write_u32::<LittleEndian>(self.asset_desc_loc.size)
            .unwrap();

        cur.write_u32::<LittleEndian>(self.buffer_views_loc.offset)
            .unwrap();
        cur.write_u32::<LittleEndian>(self.buffer_views_loc.size)
            .unwrap();

        cur.write_u32::<LittleEndian>(self.buffer_loc.offset)
            .unwrap();
        cur.write_u32::<LittleEndian>(self.buffer_loc.size).unwrap();

        cur.write_u32::<LittleEndian>(self.descriptor_loc.offset)
            .unwrap();
        cur.write_u32::<LittleEndian>(self.descriptor_loc.size)
            .unwrap();

        bytes
    }
}

#[derive(Debug, Clone)]
pub struct BNLAsset {
    description: AssetDescription,
    descriptor_bytes: Vec<u8>,
    resource_chunks: Option<Vec<Vec<u8>>>,
}

#[derive(Debug, Default)]
pub struct UnpackedBNLFile {
    header: BNLHeader,
    assets: Vec<BNLAsset>,
}

#[derive(Debug, Default)]
pub struct BNLFile {
    header: BNLHeader,

    asset_desc_bytes: Vec<u8>,
    buffer_views_bytes: Vec<u8>,
    buffer_bytes: Vec<u8>,
    descriptor_bytes: Vec<u8>,

    asset_descriptions: Vec<AssetDescription>,
}

impl UnpackedBNLFile {
    /**
    Parses a BNL file in memory, loading embedded [`AssetDescription`] data.

    # Errors
    - [`BNLError::DecompressionFailure`] when the zlib compression section of the file could not be parsed
    - [`BNLError::DataReadError`] when any other part of the file could not be parsed

    # Examples
    ```
    use bnl::BNLFile;
    use std::path::PathBuf;

    let path = PathBuf::new("./my_bnl.bnl");
    let bytes = fs::read(&path).expect("Unable to read BNL.");

    let bnl = BNLFile::from_bytes(&bytes).expect("Unable to parse BNL.");
    ```
    */
    pub fn from_bytes(bnl_bytes: &[u8]) -> Result<UnpackedBNLFile, BNLError> {
        let mut bytes = bnl_bytes[..40].to_vec();

        let mut cur = Cursor::new(bnl_bytes);

        let mut header = BNLHeader {
            file_count: cur.read_u16::<LittleEndian>()?,
            flags: cur.read_u8()?,
            ..Default::default()
        };

        cur.read_exact(&mut header.unknown_2)?;

        header.asset_desc_loc = DataView::from_cursor(&mut cur)?;
        header.buffer_views_loc = DataView::from_cursor(&mut cur)?;
        header.buffer_loc = DataView::from_cursor(&mut cur)?;
        header.descriptor_loc = DataView::from_cursor(&mut cur)?;

        let decompressed_bytes = miniz_oxide::inflate::decompress_to_vec_zlib(&bnl_bytes[40..])?;
        bytes.extend_from_slice(&decompressed_bytes);

        cur = Cursor::new(&bytes);

        let mut new_bnl = UnpackedBNLFile {
            header,
            ..Default::default()
        };

        let num_descriptions = new_bnl.header.asset_desc_loc.size as usize / ASSET_DESCRIPTION_SIZE;

        let mut asset_desc_bytes = Vec::new();
        let mut buffer_views_bytes = Vec::new();
        let mut buffer_bytes = Vec::new();
        let mut descriptor_bytes = Vec::new();

        let loc = &new_bnl.header.asset_desc_loc;
        cur.seek(SeekFrom::Start(loc.offset.into()))?;
        asset_desc_bytes.resize(loc.size as usize, 0);
        cur.read_exact(&mut asset_desc_bytes)?;

        let loc = &new_bnl.header.buffer_views_loc;
        cur.seek(SeekFrom::Start(loc.offset.into()))?;
        buffer_views_bytes.resize(loc.size as usize, 0);
        cur.read_exact(&mut buffer_views_bytes)?;

        let loc = &new_bnl.header.buffer_loc;
        cur.seek(SeekFrom::Start(loc.offset.into()))?;
        buffer_bytes.resize(loc.size as usize, 0);
        cur.read_exact(&mut buffer_bytes)?;

        let loc = &new_bnl.header.descriptor_loc;
        cur.seek(SeekFrom::Start(loc.offset.into()))?;
        descriptor_bytes.resize(loc.size as usize, 0);
        cur.read_exact(&mut descriptor_bytes)?;

        cur.seek(SeekFrom::Start(new_bnl.header.asset_desc_loc.offset as u64))?;

        for i in 0..num_descriptions {
            let mut bytes = [0x00; ASSET_DESCRIPTION_SIZE];
            cur.read_exact(&mut bytes)?;

            let mut description = AssetDescription::from_bytes(&bytes)?;
            description.asset_desc_index = i;

            let desc_start: usize = description.descriptor_ptr as usize;
            let desc_end: usize = desc_start + description.descriptor_size as usize;
            let desc_bytes = descriptor_bytes[desc_start..desc_end].to_vec();

            let resource_chunks: Option<Vec<Vec<u8>>> = match description.resource_size {
                0 => None,
                _size => Some(
                    DataViewList::from_bytes(
                        &buffer_views_bytes[description.dataview_list_ptr as usize..],
                    )
                    .map_err(|_| {
                        BNLError::DataReadError("Unable to read BufferViews.".to_string())
                    })?
                    .slices(&buffer_bytes)?
                    .iter()
                    .map(|slice| slice.to_vec())
                    .collect(),
                ),
            };

            // TODO: Resize this then push into it
            new_bnl.assets.push(BNLAsset {
                description,
                descriptor_bytes: desc_bytes,
                resource_chunks,
            });
        }

        Ok(new_bnl)
    }

    pub fn to_bytes(&mut self) -> Vec<u8> {
        let mut asset_desc_section: Vec<u8> =
            vec![0x00; ASSET_DESCRIPTION_SIZE * self.assets.len()];
        let mut buffer_views_section: Vec<u8> = vec![];
        let mut buffer_section: Vec<u8> = vec![];
        let mut descriptors_section: Vec<u8> = vec![];

        for (i, asset) in self.assets.iter().enumerate() {
            let mut asset_desc = asset.description.clone();

            if let Some(chunks) = &asset.resource_chunks {
                let num_chunks = chunks.len();

                let dvl = DataViewList {
                    size: (8 + 8 * num_chunks) as u32,
                    num_views: num_chunks as u32,
                    views: chunks
                        .iter()
                        .map(|chunk| {
                            let offset = buffer_section.len();
                            buffer_section.write_all(chunk);

                            DataView {
                                offset: offset as u32,
                                size: chunk.len() as u32,
                            }
                        })
                        .collect(),
                };

                let dvl_bytes = dvl.to_bytes();

                // Write buffer view information into asset desc
                asset_desc.dataview_list_ptr = buffer_views_section.len() as u32;
                asset_desc.resource_size = dvl.bytes_required() as u32;
                buffer_views_section
                    .write_all(&dvl_bytes)
                    .expect("Unable to write buffer view.");
            }

            asset_desc.descriptor_ptr = descriptors_section.len() as u32;
            asset_desc.descriptor_size = asset.descriptor_bytes.len() as u32;
            descriptors_section.extend_from_slice(&asset.descriptor_bytes);

            let start = i * ASSET_DESCRIPTION_SIZE;
            let end = start + ASSET_DESCRIPTION_SIZE;

            asset_desc_section[start..end].copy_from_slice(&asset_desc.to_bytes());
        }

        let asset_desc_offset: usize = 40;
        let asset_desc_size: usize = asset_desc_section.len();

        let buffer_views_offset: usize = asset_desc_offset + asset_desc_size;
        let buffer_views_size: usize = buffer_views_section.len();

        let buffer_offset: usize = buffer_views_offset + buffer_views_size;
        let buffer_size: usize = buffer_section.len();

        let descriptors_offset: usize = buffer_offset + buffer_size;
        let descriptors_size: usize = descriptors_section.len();

        let new_header = BNLHeader {
            file_count: self.assets.len() as u16,
            asset_desc_loc: DataView {
                offset: asset_desc_offset as u32,
                size: asset_desc_size as u32,
            },
            buffer_views_loc: DataView {
                offset: buffer_views_offset as u32,
                size: buffer_views_size as u32,
            },
            buffer_loc: DataView {
                offset: buffer_offset as u32,
                size: buffer_size as u32,
            },
            descriptor_loc: DataView {
                offset: descriptors_offset as u32,
                size: descriptors_size as u32,
            },
            ..self.header
        };

        self.header = new_header;

        let mut decompressed_bytes = Vec::new();

        decompressed_bytes.extend_from_slice(&asset_desc_section);
        decompressed_bytes.extend_from_slice(&buffer_views_section);
        decompressed_bytes.extend_from_slice(&buffer_section);
        decompressed_bytes.extend_from_slice(&descriptors_section);

        let compressed_bytes = miniz_oxide::deflate::compress_to_vec_zlib(&decompressed_bytes, 1);

        let mut bytes = vec![0; compressed_bytes.len() + 40];

        bytes[0..40].copy_from_slice(&self.header.to_bytes());
        bytes[40..].copy_from_slice(&compressed_bytes);

        bytes
    }

    /// Retrieves an asset by name and type, creating it from the bytes of the BNL file.
    ///
    /// # Errors
    /// - [`AssetError::NotFound`] when the given name can't be found
    /// - [`AssetError::TypeMismatch`] when the asset is found, but doesn't match the requested type
    /// - [`AssetError::ParseError`] when the asset is found, the type matches but an error occurs while parsing the asset
    ///
    /// # Examples
    /// ```
    /// use bnl::BNLFile;
    /// use bnl::asset::Texture;
    ///
    /// let bnl_file = BNLFile::from_bytes(...);
    /// let tex = bnl_file.get_asset::<Texture>("aid_texture_mytexture_a_b")
    ///                   .expect("Unable to get texture.");
    /// ```
    pub fn get_asset<A: Asset>(&self, name: &str) -> Result<A, AssetError> {
        for asset in &self.assets {
            let asset_desc = &asset.description;

            if asset_desc.name() == name {
                if asset_desc.asset_type() != A::asset_type() {
                    return Err(AssetError::TypeMismatch);
                }

                let descriptor = A::Descriptor::from_bytes(&asset.descriptor_bytes)?;

                let slices: Vec<&[u8]> = match &asset.resource_chunks {
                    Some(slices) => slices.iter().map(|slice| slice.as_ref()).collect(),
                    None => vec![],
                };

                let vr = VirtualResource::from_slices(&slices);

                let asset = A::new(asset_desc, &descriptor, &vr)?;

                return Ok(asset);
            }
        }

        Err(AssetError::NotFound)
    }

    /// Returns all assets of a given type from this [`BNLFile`].
    ///
    /// # Examples
    ///
    /// ```
    /// use bnl::BNLFile;
    /// use bnl::asset::Texture;
    ///
    /// let bnl_file = BNLFile::from_bytes(...);
    /// let textures = bnl_file.get_assets::<Texture>();
    ///
    /// // Dump all of the textures here
    /// ```
    pub fn get_assets<A: Asset>(&self) -> Vec<A> {
        let mut assets = Vec::new();

        for asset in &self.assets {
            let asset_desc = &asset.description;

            if asset_desc.asset_type() != A::asset_type() {
                continue;
            }

            if let Ok(descriptor) = A::Descriptor::from_bytes(&asset.descriptor_bytes) {
                let slices: Vec<&[u8]> = match &asset.resource_chunks {
                    Some(slices) => slices.iter().map(|slice| slice.as_ref()).collect(),
                    None => vec![],
                };

                let vr = VirtualResource::from_slices(&slices);

                if let Ok(asset) = A::new(asset_desc, &descriptor, &vr) {
                    assets.push(asset);
                }
            }
        }

        assets
    }

    /// Retrieves a [`RawAsset`] by name, or None if it can't be found.
    ///
    /// # Examples
    /// ```
    /// use bnl::BNLFile;
    /// use bnl::asset::Texture;
    ///
    /// let bnl_file = BNLFile::from_bytes(...);
    /// let raw_asset = bnl_file.get_raw_asset().expect("Unable to extract asset.");
    ///
    /// // Dump the data from the RawAsset
    /// std::fs::write("./descriptor", &raw_asset.descriptor_bytes).expect("Unable to write
    /// descriptor.");
    /// raw_asset.data_slices.iter().enumerate().for_each(|(i, slice)| {
    ///     std::fs::write(format!("./resource{}", i), &slice).expect("Unable to write resource.");
    /// });
    /// ```
    pub fn get_raw_asset(&self, name: &str) -> Option<&BNLAsset> {
        for asset in &self.assets {
            if asset.description.name() == name {
                return Some(asset);
            }
        }

        None
    }

    /*
    pub fn get_overlaps(&self) -> Result<Vec<Range<usize>>, BNLError> {
        let mut dvls = Vec::with_capacity(self.asset_descriptions().len());

        self.asset_descriptions()
            .iter()
            .filter(|asset_desc| asset_desc.dataview_list_ptr != 0)
            .map(|asset_desc| {
                DataViewList::from_bytes(
                    &self.buffer_views_bytes[asset_desc.dataview_list_ptr as usize..],
                )
            });

        for asset_desc in self.asset_descriptions() {
            if asset_desc.dataview_list_ptr != 0 {
                dvls.push(
                    DataViewList::from_bytes(
                        &self.buffer_views_bytes[asset_desc.dataview_list_ptr as usize..],
                    )
                    .map_err(|_| {
                        BNLError::DataReadError(format!(
                            "Unable to read Data View List for asset {}",
                            asset_desc.name()
                        ))
                    })?,
                );
            }
        }

        for pair in dvls.iter().zip(&dvls) {
            if std::ptr::eq(pair.0, pair.1) {
                continue;
            }
        }

        Ok(vec![])
    }
    */

    /// Retrieves all [`RawAsset`] entries.
    ///
    /// # Examples
    /// ```
    /// use bnl::BNLFile;
    /// use bnl::asset::Texture;
    ///
    /// let bnl_file = BNLFile::from_bytes(...);
    /// let raw_assets = bnl_file.get_raw_assets().expect("Unable to extract.");
    ///
    /// // Dump the data from the RawAsset
    ///
    /// for raw_asset in raw_assets {
    ///     std::fs::write("./descriptor", &raw_asset.descriptor_bytes)
    ///                         .expect("Unable to write descriptor.");
    ///
    ///     raw_asset.data_slices.iter().enumerate().for_each(|(i, slice)| {
    ///         std::fs::write(format!("./resource{}", i), &slice)
    ///                         .expect("Unable to write resource.");;
    ///     });
    /// }
    /// ```
    pub fn get_raw_assets(&self) -> &Vec<BNLAsset> {
        &self.assets
    }

    pub fn update_asset(&mut self, name: &str, bnl_asset: &BNLAsset) -> Result<(), AssetError> {
        for asset in &mut self.assets {
            if asset.description.name() == name {
                *asset = bnl_asset.clone();

                return Ok(());
            }
        }

        Err(AssetError::NotFound)
    }

    pub fn get_assets_occupying_descriptor_range(
        &self,
        range: Range<usize>,
    ) -> Vec<&AssetDescription> {
        self.assets
            .iter()
            .map(|asset| &asset.description)
            .filter(|asset_desc| {
                let start1 = range.start;
                let end1 = range.end;

                let start2 = asset_desc.descriptor_ptr as usize;
                let end2 = start2 + asset_desc.descriptor_size as usize;

                start1 < end2 && start2 < end1
            })
            .collect()
    }
}

impl BNLFile {
    /**
    Parses a BNL file in memory, loading embedded [`AssetDescription`] data.

    # Errors
    - [`BNLError::DecompressionFailure`] when the zlib compression section of the file could not be parsed
    - [`BNLError::DataReadError`] when any other part of the file could not be parsed

    # Examples
    ```
    use bnl::BNLFile;
    use std::path::PathBuf;

    let path = PathBuf::new("./my_bnl.bnl");
    let bytes = fs::read(&path).expect("Unable to read BNL.");

    let bnl = BNLFile::from_bytes(&bytes).expect("Unable to parse BNL.");
    ```
    */
    pub fn from_bytes(bnl_bytes: &[u8]) -> Result<BNLFile, BNLError> {
        let mut bytes = bnl_bytes[..40].to_vec();

        let mut cur = Cursor::new(bnl_bytes);

        let mut header = BNLHeader {
            file_count: cur.read_u16::<LittleEndian>()?,
            flags: cur.read_u8()?,
            ..Default::default()
        };

        cur.read_exact(&mut header.unknown_2)?;

        header.asset_desc_loc = DataView::from_cursor(&mut cur)?;
        header.buffer_views_loc = DataView::from_cursor(&mut cur)?;
        header.buffer_loc = DataView::from_cursor(&mut cur)?;
        header.descriptor_loc = DataView::from_cursor(&mut cur)?;

        let decompressed_bytes = miniz_oxide::inflate::decompress_to_vec_zlib(&bnl_bytes[40..])?;
        bytes.extend_from_slice(&decompressed_bytes);

        cur = Cursor::new(&bytes);

        let mut new_bnl = BNLFile {
            header,
            ..Default::default()
        };

        let num_descriptions = new_bnl.header.asset_desc_loc.size as usize / ASSET_DESCRIPTION_SIZE;

        cur.seek(SeekFrom::Start(new_bnl.header.asset_desc_loc.offset as u64))?;

        for i in 0..num_descriptions {
            let mut bytes = [0x00; ASSET_DESCRIPTION_SIZE];
            cur.read_exact(&mut bytes)?;

            // TODO: Rework this into an actual constructor
            let mut asset_desc = AssetDescription::from_bytes(&bytes)?;
            asset_desc.asset_desc_index = i;

            // TODO: Resize this then push into it
            new_bnl.asset_descriptions.push(asset_desc);
        }

        let loc = &new_bnl.header.asset_desc_loc;
        cur.seek(SeekFrom::Start(loc.offset.into()))?;
        new_bnl.asset_desc_bytes.resize(loc.size as usize, 0);
        cur.read_exact(&mut new_bnl.asset_desc_bytes)?;

        let loc = &new_bnl.header.buffer_views_loc;
        cur.seek(SeekFrom::Start(loc.offset.into()))?;
        new_bnl.buffer_views_bytes.resize(loc.size as usize, 0);
        cur.read_exact(&mut new_bnl.buffer_views_bytes)?;

        let loc = &new_bnl.header.buffer_loc;
        cur.seek(SeekFrom::Start(loc.offset.into()))?;
        new_bnl.buffer_bytes.resize(loc.size as usize, 0);
        cur.read_exact(&mut new_bnl.buffer_bytes)?;

        let loc = &new_bnl.header.descriptor_loc;
        cur.seek(SeekFrom::Start(loc.offset.into()))?;
        new_bnl.descriptor_bytes.resize(loc.size as usize, 0);
        cur.read_exact(&mut new_bnl.descriptor_bytes)?;

        Ok(new_bnl)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut decompressed_bytes = Vec::new();

        decompressed_bytes.extend_from_slice(&self.asset_desc_bytes);
        decompressed_bytes.extend_from_slice(&self.buffer_views_bytes);
        decompressed_bytes.extend_from_slice(&self.buffer_bytes);
        decompressed_bytes.extend_from_slice(&self.descriptor_bytes);

        let compressed_bytes = miniz_oxide::deflate::compress_to_vec_zlib(&decompressed_bytes, 1);

        let mut bytes = vec![0; compressed_bytes.len() + 40];

        bytes[0..40].copy_from_slice(&self.header.to_bytes());
        bytes[40..].copy_from_slice(&compressed_bytes);

        bytes
    }

    /// Retrieves an asset by name and type, creating it from the bytes of the BNL file.
    ///
    /// # Errors
    /// - [`AssetError::NotFound`] when the given name can't be found
    /// - [`AssetError::TypeMismatch`] when the asset is found, but doesn't match the requested type
    /// - [`AssetError::ParseError`] when the asset is found, the type matches but an error occurs while parsing the asset
    ///
    /// # Examples
    /// ```
    /// use bnl::BNLFile;
    /// use bnl::asset::Texture;
    ///
    /// let bnl_file = BNLFile::from_bytes(...);
    /// let tex = bnl_file.get_asset::<Texture>("aid_texture_mytexture_a_b")
    ///                   .expect("Unable to get texture.");
    /// ```
    pub fn get_asset<A: Asset>(&self, name: &str) -> Result<A, AssetError> {
        for asset_desc in &self.asset_descriptions {
            if asset_desc.name() == name {
                if asset_desc.asset_type() != A::asset_type() {
                    return Err(AssetError::TypeMismatch);
                }

                let descriptor_ptr: usize = asset_desc.descriptor_ptr() as usize;
                let desc_slice = &self.descriptor_bytes[descriptor_ptr..];

                let descriptor: A::Descriptor = A::Descriptor::from_bytes(desc_slice)?;

                let dvl = self
                    .get_dataview_list(asset_desc.dataview_list_ptr as usize)
                    .map_err(|_| {
                        AssetError::ParseError(AssetParseError::InvalidDataViews(
                            "Unable to get data view list from BNL data.".to_string(),
                        ))
                    })?;

                let virtual_res =
                    VirtualResource::from_dvl(&dvl, &self.buffer_bytes).map_err(|e| {
                        AssetError::ParseError(AssetParseError::InvalidDataViews(format!(
                            "Unable to get data from data slices.\nError: {}",
                            e
                        )))
                    })?;

                let asset = A::new(asset_desc, &descriptor, &virtual_res)?;

                return Ok(asset);
            }
        }

        Err(AssetError::NotFound)
    }

    /// Returns all assets of a given type from this [`BNLFile`].
    ///
    /// # Examples
    ///
    /// ```
    /// use bnl::BNLFile;
    /// use bnl::asset::Texture;
    ///
    /// let bnl_file = BNLFile::from_bytes(...);
    /// let textures = bnl_file.get_assets::<Texture>();
    ///
    /// // Dump all of the textures here
    /// ```
    pub fn get_assets<A: Asset>(&self) -> Vec<A> {
        let mut assets = Vec::new();

        for asset_desc in &self.asset_descriptions {
            if asset_desc.asset_type() != A::asset_type() {
                continue;
            }

            let descriptor_ptr: usize = asset_desc.descriptor_ptr() as usize;
            let desc_slice = &self.descriptor_bytes[descriptor_ptr..];

            let descriptor: A::Descriptor = match A::Descriptor::from_bytes(desc_slice) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!(
                        "Error getting asset descriptor for {}\nError: {}",
                        asset_desc.name(),
                        e
                    );
                    continue;
                }
            };

            let dvl = match self.get_dataview_list(asset_desc.dataview_list_ptr as usize) {
                Ok(dvl) => dvl,
                Err(_) => {
                    continue;
                }
            };

            let virtual_res = match VirtualResource::from_dvl(&dvl, &self.buffer_bytes) {
                Ok(res) => res,
                Err(_) => {
                    continue;
                }
            };

            match A::new(asset_desc, &descriptor, &virtual_res) {
                Ok(a) => assets.push(a),
                Err(e) => eprintln!(
                    "Failed to load asset \"{}\"\n    Error: {}",
                    asset_desc.name(),
                    e
                ),
            };
        }

        assets
    }

    /// Retrieves a [`RawAsset`] by name.
    ///
    /// # Errors
    /// Returns an [`AssetError`] if the asset can not be parsed from the [`BNLFile`].
    ///
    /// # Examples
    /// ```
    /// use bnl::BNLFile;
    /// use bnl::asset::Texture;
    ///
    /// let bnl_file = BNLFile::from_bytes(...);
    /// let raw_asset = bnl_file.get_raw_asset().expect("Unable to extract.");
    ///
    /// // Dump the data from the RawAsset
    /// std::fs::write("./descriptor", &raw_asset.descriptor_bytes).expect("Unable to write
    /// descriptor.");
    /// raw_asset.data_slices.iter().enumerate().for_each(|(i, slice)| {
    ///     std::fs::write(format!("./resource{}", i), &slice).expect("Unable to write resource.");
    /// });
    /// ```
    pub fn get_raw_asset(&self, name: &str) -> Result<RawAsset, AssetError> {
        for asset_desc in &self.asset_descriptions {
            if asset_desc.name() == name {
                let desc_ptr: usize = asset_desc.descriptor_ptr() as usize;
                let desc_size: usize = asset_desc.descriptor_size as usize;

                let desc_bytes: Vec<u8> =
                    self.descriptor_bytes[desc_ptr..desc_ptr + desc_size].to_vec();

                /*
                    .map_err(|e| {
                        AssetError::AssetParseError(AssetParseError::InvalidDataViews(
                            "bruh".to_string(),
                        ))
                    })?;
                */

                let dvl = self
                    .get_dataview_list(asset_desc.dataview_list_ptr as usize)
                    .map_err(|_| {
                        AssetError::ParseError(AssetParseError::InvalidDataViews(
                            "Unable to get data view list from BNL data.".to_string(),
                        ))
                    })?;

                let slices = dvl.slices(&self.buffer_bytes).map_err(|_| {
                    AssetError::ParseError(AssetParseError::InvalidDataViews(
                        "Unable to get data from data slices.".to_string(),
                    ))
                })?;

                return Ok(RawAsset {
                    name: asset_desc.name().to_string(),
                    asset_type: asset_desc.asset_type,
                    descriptor_bytes: desc_bytes,
                    data_slices: slices.iter().map(|s| s.to_vec()).collect(),
                });
            }
        }

        Err(AssetError::NotFound)
    }

    pub fn get_overlaps(&self) -> Result<Vec<Range<usize>>, BNLError> {
        let mut dvls = Vec::with_capacity(self.asset_descriptions().len());

        self.asset_descriptions()
            .iter()
            .filter(|asset_desc| asset_desc.dataview_list_ptr != 0)
            .map(|asset_desc| {
                DataViewList::from_bytes(
                    &self.buffer_views_bytes[asset_desc.dataview_list_ptr as usize..],
                )
            });

        for asset_desc in self.asset_descriptions() {
            if asset_desc.dataview_list_ptr != 0 {
                dvls.push(
                    DataViewList::from_bytes(
                        &self.buffer_views_bytes[asset_desc.dataview_list_ptr as usize..],
                    )
                    .map_err(|_| {
                        BNLError::DataReadError(format!(
                            "Unable to read Data View List for asset {}",
                            asset_desc.name()
                        ))
                    })?,
                );
            }
        }

        for pair in dvls.iter().zip(&dvls) {
            if std::ptr::eq(pair.0, pair.1) {
                continue;
            }
        }

        Ok(vec![])
    }

    /// Retrieves all [`RawAsset`] entries.
    ///
    /// # Examples
    /// ```
    /// use bnl::BNLFile;
    /// use bnl::asset::Texture;
    ///
    /// let bnl_file = BNLFile::from_bytes(...);
    /// let raw_assets = bnl_file.get_raw_assets().expect("Unable to extract.");
    ///
    /// // Dump the data from the RawAsset
    ///
    /// for raw_asset in raw_assets {
    ///     std::fs::write("./descriptor", &raw_asset.descriptor_bytes)
    ///                         .expect("Unable to write descriptor.");
    ///
    ///     raw_asset.data_slices.iter().enumerate().for_each(|(i, slice)| {
    ///         std::fs::write(format!("./resource{}", i), &slice)
    ///                         .expect("Unable to write resource.");;
    ///     });
    /// }
    /// ```
    pub fn get_raw_assets(&self) -> Vec<RawAsset> {
        let mut assets = Vec::new();

        let clo = |asset_desc: &AssetDescription| -> Result<RawAsset, AssetError> {
            let desc_ptr: usize = asset_desc.descriptor_ptr() as usize;
            let desc_size: usize = asset_desc.descriptor_size as usize;

            let desc_bytes: Vec<u8> =
                self.descriptor_bytes[desc_ptr..desc_ptr + desc_size].to_vec();

            let dvl = self
                .get_dataview_list(asset_desc.dataview_list_ptr as usize)
                .map_err(|_| {
                    AssetError::ParseError(AssetParseError::InvalidDataViews(
                        "Unable to get data view list from BNL data.".to_string(),
                    ))
                })?;

            let slices = dvl.slices(&self.buffer_bytes).map_err(|_| {
                AssetError::ParseError(AssetParseError::InvalidDataViews(
                    "Unable to get data from data slices.".to_string(),
                ))
            })?;

            Ok(RawAsset {
                name: asset_desc.name().to_string(),
                asset_type: asset_desc.asset_type,
                descriptor_bytes: desc_bytes,
                data_slices: slices.iter().map(|s| s.to_vec()).collect(),
            })
        };

        for asset_desc in &self.asset_descriptions {
            match clo(asset_desc) {
                Ok(asset) => {
                    assets.push(asset);
                }
                Err(e) => {
                    eprintln!(
                        "Error retrieving RawAsset for {}.\nError: {}",
                        asset_desc.name(),
                        e
                    );
                }
            }
        }

        assets
    }

    pub fn update_asset_from_descriptor<AD: AssetDescriptor>(
        &mut self,
        name: &str,
        descriptor: &AD,
        data: Option<&Vec<u8>>,
    ) -> Result<(), AssetError> {
        let mut asset_desc = self
            .get_asset_description(name)
            .ok_or(AssetError::NotFound)?
            .clone();

        if asset_desc.asset_type() != AD::asset_type() {
            return Err(AssetError::TypeMismatch);
        }

        // Update the descriptor
        let prev_descriptor: AD = self.get_descriptor(name)?;

        let new_size = descriptor.size();
        let prev_size = prev_descriptor.size();

        if new_size > prev_size {
            let start = asset_desc.descriptor_ptr as usize;
            let end = start + new_size;

            // TODO: Actually check for overlaps
            let _occupants = self.get_assets_occupying_descriptor_range(start..end);

            return Err(AssetError::ParseError(AssetParseError::InvalidDataViews(
                "The descriptor can not grow in size. (WIP to allow descriptor growing.)"
                    .to_string(),
            )));
        }

        asset_desc.descriptor_size = new_size as u32;

        let start: usize = asset_desc.descriptor_ptr as usize;
        let end: usize = start + new_size;

        // Update the descriptor section
        self.descriptor_bytes[start..end].copy_from_slice(&descriptor.to_bytes()?);

        // Update the dvl and resource sections
        if let Some(data) = data {
            let dvl = self
                .get_dataview_list(asset_desc.dataview_list_ptr as usize)
                .map_err(|_| {
                    AssetError::ParseError(AssetParseError::InvalidDataViews(
                        "Unable to get data view list from BNL data.".to_string(),
                    ))
                })?;

            // TODO: Update the DVL section
            dvl.write_bytes(data, &mut self.buffer_bytes)
                .map_err(|_| AssetError::ParseError(AssetParseError::ErrorParsingDescriptor))?;
        }

        // Update the asset descriptions section
        self.update_asset_description(&asset_desc)?;

        Ok(())
    }

    pub fn get_asset_description(&self, name: &str) -> Option<&AssetDescription> {
        self.asset_descriptions
            .iter()
            .find(|asset_desc| asset_desc.name() == name)
    }

    pub fn update_asset_description(
        &mut self,
        asset_desc: &AssetDescription,
    ) -> Result<(), AssetError> {
        let start: usize = asset_desc.asset_desc_index * ASSET_DESCRIPTION_SIZE;
        let end: usize = start + ASSET_DESCRIPTION_SIZE;

        self.asset_desc_bytes[start..end].copy_from_slice(&asset_desc.to_bytes());

        Ok(())
    }

    pub fn get_descriptor<AD: AssetDescriptor>(&self, name: &str) -> Result<AD, AssetError> {
        for asset_desc in &self.asset_descriptions {
            if asset_desc.name() == name {
                if asset_desc.asset_type() != AD::asset_type() {
                    return Err(AssetError::TypeMismatch);
                }

                let descriptor_ptr: usize = asset_desc.descriptor_ptr() as usize;
                let desc_slice = &self.descriptor_bytes[descriptor_ptr..];

                let descriptor = AD::from_bytes(desc_slice)?;

                return Ok(descriptor);
            }
        }

        Err(AssetError::NotFound)
    }

    pub fn get_assets_occupying_descriptor_range(
        &self,
        range: Range<usize>,
    ) -> Vec<&AssetDescription> {
        self.asset_descriptions()
            .iter()
            .filter(|asset_desc| {
                let start1 = range.start;
                let end1 = range.end;

                let start2 = asset_desc.descriptor_ptr as usize;
                let end2 = start2 + asset_desc.descriptor_size as usize;

                start1 < end2 && start2 < end1
            })
            .collect()
    }

    /// Returns a reference to the asset descriptions of this [`BNLFile`].
    pub fn asset_descriptions(&self) -> &[AssetDescription] {
        &self.asset_descriptions
    }

    fn get_dataview_list(&self, offset: usize) -> Result<DataViewList, Box<dyn Error>> {
        Ok(DataViewList::from_bytes(
            &self.buffer_views_bytes[offset..],
        )?)
    }
}

#[derive(Debug)]
pub struct VirtualResource<'a> {
    slices: Vec<&'a [u8]>,
}

#[derive(Debug)]
pub enum VirtualResourceError {
    OffsetOutOfBounds,
    SizeOutOfBounds,
}

impl Display for VirtualResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl VirtualResource<'_> {
    pub(crate) fn from_dvl<'a>(
        dataview_list: &DataViewList,
        bytes: &'a [u8],
    ) -> Result<VirtualResource<'a>, VirtualResourceError> {
        let views = dataview_list.views();

        let mut slices = Vec::new();

        for view in views {
            let offset = view.offset as usize;
            let size = view.size as usize;

            if offset > bytes.len() {
                return Err(VirtualResourceError::OffsetOutOfBounds);
            } else if bytes.len() - offset < size {
                return Err(VirtualResourceError::SizeOutOfBounds);
            }

            slices.push(&bytes[offset..offset + size]);
        }

        Ok(VirtualResource { slices })
    }

    pub fn get_bytes(
        &self,
        start_offset: usize,
        get_size: usize,
    ) -> Result<Vec<u8>, VirtualResourceError>
where {
        let end = self.len();

        if end < start_offset {
            return Err(VirtualResourceError::OffsetOutOfBounds);
        } else if end - start_offset < get_size {
            return Err(VirtualResourceError::SizeOutOfBounds);
        }

        let mut v = vec![0; get_size];

        let mut slice_start = 0usize;
        let mut total_written = 0usize;

        for slice in &self.slices {
            let slice_size = slice.len();

            // If this slice is part of the copy in any way
            if (slice_start + slice_size) > start_offset {
                let desired_cp_size = get_size - total_written;

                // Get start index
                let cp_i = start_offset.saturating_sub(slice_start);
                let cp_size = cmp::min(desired_cp_size, slice_size - cp_i);

                let cp_j = cp_i + cp_size;

                v[total_written..total_written + cp_size].copy_from_slice(&slice[cp_i..cp_j]);

                total_written += cp_size;

                if total_written > get_size {
                    return Err(VirtualResourceError::SizeOutOfBounds);
                } else if total_written == get_size {
                    break;
                }
            }

            slice_start += slice_size;
        }

        if total_written != get_size {
            return Err(VirtualResourceError::SizeOutOfBounds);
        }

        Ok(v)
    }

    pub fn get_all_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0x00; self.len()];

        let mut curr = 0usize;
        for slice in &self.slices {
            let copy_size = slice.len();

            bytes[curr..curr + copy_size].copy_from_slice(slice);

            curr += copy_size;
        }

        bytes
    }

    pub(crate) fn from_slices<'a>(slices: &'a [&[u8]]) -> VirtualResource<'a> {
        VirtualResource {
            slices: slices.to_vec(),
        }
    }

    pub fn len(&self) -> usize {
        self.slices
            .iter()
            .fold(0, |acc, slice: &&[u8]| -> usize { acc + (*slice).len() })
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn slices(&self) -> &[&[u8]] {
        &self.slices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn make_data<const N: usize>() -> [u8; N] {
        let mut arr = [0u8; N];
        let mut i = 0;
        while i < N {
            arr[i] = i as u8;
            i += 1;
        }

        arr
    }

    const DATA: [u8; 1000] = make_data::<1000>();

    #[test]
    fn read_across_slices() {
        let slices = [
            &DATA[0..100],
            &DATA[200..300],
            &DATA[400..500],
            &DATA[600..700],
        ];

        let virtual_res = VirtualResource::from_slices(&slices);

        let bytes = virtual_res.get_bytes(180, 200).unwrap();

        assert_eq!(bytes[0..20], DATA[280..300]);
        assert_eq!(bytes[20..120], DATA[400..500]);
        assert_eq!(bytes[120..200], DATA[600..680]);
    }
}
