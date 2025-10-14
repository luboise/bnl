use std::{
    io::{Cursor, Read, Seek, SeekFrom, Write},
    ops::Range,
};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::{
    VirtualResource,
    asset::{
        ASSET_DESCRIPTION_SIZE, Asset, AssetDescription, AssetDescriptor, AssetError, AssetLike,
        AssetName, AssetType, DataViewList,
    },
};

#[derive(Debug, Default)]
pub struct BNLFile {
    header: BNLHeader,
    assets: Vec<RawAsset>,
}

#[derive(Debug, Default)]
pub struct BNLHeader {
    pub(crate) file_count: u16,
    pub(crate) flags: u8,
    pub(crate) unknown_2: [u8; 5],

    pub(crate) asset_desc_loc: DataView,
    pub(crate) buffer_views_loc: DataView,
    pub(crate) buffer_loc: DataView,
    pub(crate) descriptor_loc: DataView,
}

#[derive(Debug, Copy, Clone, Default)]
pub struct DataView {
    pub(crate) offset: u32,
    pub(crate) size: u32,
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
pub struct AssetMetadata {
    pub name: AssetName,
    pub asset_type: AssetType,
    pub unk_1: u32,
    pub unk_2: u32,
}

impl From<AssetDescription> for AssetMetadata {
    fn from(value: AssetDescription) -> Self {
        value.metadata.clone()
    }
}

impl From<AssetMetadata> for AssetDescription {
    fn from(value: AssetMetadata) -> Self {
        Self {
            metadata: value,
            chunk_count: 2,

            descriptor_ptr: 0,
            descriptor_size: 0,
            dataview_list_ptr: 0,
            resource_size: 0,
        }
    }
}

impl AssetMetadata {
    pub fn new(name: AssetName, asset_type: AssetType, unk_1: u32, unk_2: u32) -> Self {
        Self {
            name,
            asset_type,
            unk_1,
            unk_2,
        }
    }

    pub fn name(&self) -> &str {
        std::str::from_utf8(&self.name)
            .unwrap_or("")
            .split('\0')
            .next()
            .unwrap_or("")
    }

    pub fn asset_type(&self) -> AssetType {
        self.asset_type
    }

    pub fn unk_1(&self) -> u32 {
        self.unk_1
    }
}

#[derive(Debug, Clone)]
pub struct RawAsset {
    metadata: AssetMetadata,
    descriptor_bytes: Vec<u8>,
    resource_chunks: Option<Vec<Vec<u8>>>,
}

impl RawAsset {
    pub fn new(
        metadata: AssetMetadata,
        descriptor_bytes: Vec<u8>,
        resource_chunks: Option<Vec<Vec<u8>>>,
    ) -> Self {
        Self {
            metadata,
            descriptor_bytes,
            resource_chunks,
        }
    }

    pub fn name(&self) -> &str {
        self.metadata.name()
    }

    pub fn metadata(&self) -> &AssetMetadata {
        &self.metadata
    }
    pub fn metadata_mut(&mut self) -> &mut AssetMetadata {
        &mut self.metadata
    }

    pub fn descriptor_bytes(&self) -> &[u8] {
        &self.descriptor_bytes
    }
    pub fn descriptor_bytes_mut(&mut self) -> &mut Vec<u8> {
        &mut self.descriptor_bytes
    }

    pub fn resource_chunks(&self) -> Option<&Vec<Vec<u8>>> {
        self.resource_chunks.as_ref()
    }
    pub fn resource_chunks_mut(&mut self) -> &mut Option<Vec<Vec<u8>>> {
        &mut self.resource_chunks
    }

    pub fn to_asset<AL: AssetLike>(self) -> Result<Asset<AL>, AssetError> {
        let description = &self.metadata;

        if description.asset_type() != AL::asset_type() {
            return Err(AssetError::TypeMismatch);
        }

        let descriptor = AL::Descriptor::from_bytes(&self.descriptor_bytes)?;

        let slices: Vec<&[u8]> = match &self.resource_chunks {
            Some(slices) => slices.iter().map(|slice| slice.as_ref()).collect(),
            None => vec![],
        };

        let vr = VirtualResource::from_slices(&slices);

        let asset = AL::new(&descriptor, &vr)?;

        Ok(Asset {
            metadata: description.clone(),
            asset,
        })
    }
}

impl BNLFile {
    /**
    Parses a BNL file in memory, loading embedded [`PartialAssetDescription`] data.

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
    pub fn from_bytes(bnl_bytes: &[u8]) -> Result<Self, BNLError> {
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

        let mut new_bnl = Self {
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

        for _ in 0..num_descriptions {
            let mut bytes = [0x00; ASSET_DESCRIPTION_SIZE];
            cur.read_exact(&mut bytes)?;

            let description = AssetDescription::from_bytes(&bytes)?;

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
            new_bnl.assets.push(RawAsset {
                metadata: description.metadata,
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
            let metadata = asset.metadata.clone();
            let mut asset_desc: AssetDescription = metadata.into();

            if let Some(chunks) = &asset.resource_chunks {
                let num_chunks = chunks.len();

                let dvl = DataViewList {
                    size: (8 + 8 * num_chunks) as u32,
                    num_views: num_chunks as u32,
                    views: chunks
                        .iter()
                        .map(|chunk| {
                            let offset = buffer_section.len();

                            // TODO: Find a way to propagate this, or safely ignore it
                            let _ = buffer_section.write_all(chunk);

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

    /// Retrieves an asset by name and type, converting it to the target format if it matches the
    /// format of the asset's descriptor.
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
    pub fn get_asset<AL: AssetLike>(&self, name: &str) -> Result<Asset<AL>, AssetError> {
        let raw_asset = self.get_raw_asset(name).ok_or(AssetError::NotFound)?;

        let description = &raw_asset.metadata;

        if description.asset_type() != AL::asset_type() {
            return Err(AssetError::TypeMismatch);
        }

        let descriptor = AL::Descriptor::from_bytes(&raw_asset.descriptor_bytes)?;

        let slices: Vec<&[u8]> = match &raw_asset.resource_chunks {
            Some(slices) => slices.iter().map(|slice| slice.as_ref()).collect(),
            None => vec![],
        };

        let vr = VirtualResource::from_slices(&slices);

        let asset = AL::new(&descriptor, &vr)?;

        Ok(Asset {
            metadata: description.clone(),
            asset,
        })
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
    pub fn get_assets<AL: AssetLike>(&self) -> Vec<AL> {
        let mut assets = Vec::new();

        for asset in &self.assets {
            let asset_desc = &asset.metadata;

            if asset_desc.asset_type() != AL::asset_type() {
                continue;
            }

            if let Ok(descriptor) = AL::Descriptor::from_bytes(&asset.descriptor_bytes) {
                let slices: Vec<&[u8]> = match &asset.resource_chunks {
                    Some(slices) => slices.iter().map(|slice| slice.as_ref()).collect(),
                    None => vec![],
                };

                let vr = VirtualResource::from_slices(&slices);

                if let Ok(asset) = AL::new(&descriptor, &vr) {
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
    pub fn get_raw_asset(&self, name: &str) -> Option<&RawAsset> {
        self.assets
            .iter()
            .find(|&asset| asset.metadata.name() == name)
    }

    pub(crate) fn get_raw_asset_mut(&mut self, name: &str) -> Option<&mut RawAsset> {
        self.assets
            .iter_mut()
            .find(|asset| asset.metadata.name() == name)
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
    pub fn get_raw_assets(&self) -> &Vec<RawAsset> {
        &self.assets
    }

    /*
    pub fn update_asset(&mut self, name: &str, bnl_asset: &BNLAsset) -> Result<(), AssetError> {
        for asset in &mut self.assets {
            if asset.description.name() == name {
                *asset = bnl_asset.clone();

                return Ok(());
            }
        }

        Err(AssetError::NotFound)
    }
    */

    pub fn modify_asset<AL, F>(&mut self, name: &str, f: F) -> Result<(), AssetError>
    where
        AL: AssetLike,
        F: FnOnce(&mut Asset<AL>) -> Result<(), AssetError>,
    {
        let raw_asset = self.get_raw_asset_mut(name).ok_or(AssetError::NotFound)?;

        let mut asset = raw_asset.clone().to_asset::<AL>()?;

        f(&mut asset)?;

        *raw_asset = asset.to_raw_asset()?;

        Ok(())
    }

    // TODO: Need to reimplement this for this kind of asset
    /*
    pub fn get_assets_occupying_descriptor_range(
        &self,
        range: Range<usize>,
    ) -> Vec<&AssetMetadata> {
        todo!();
    }
    */

    pub fn append_asset<AL: AssetLike>(
        &mut self,
        metadata: AssetMetadata,
        new_asset: AL,
    ) -> Result<(), AssetError> {
        self.append_raw_asset(RawAsset::new(
            metadata,
            new_asset.get_descriptor().to_bytes()?,
            new_asset.get_resource_chunks(),
        ));

        Ok(())
    }

    pub fn append_raw_asset(&mut self, new_raw_asset: RawAsset) {
        self.assets.push(new_raw_asset);
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
