use std::{
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    ops::Range,
    path::{self, Path, PathBuf},
};

use binrw::{BinReaderExt, BinWrite, BinWriterExt};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use miniz_oxide::inflate::TINFLStatus;

use crate::asset::{
    ASSET_DESCRIPTION_SIZE, Asset, AssetData, AssetDescription, AssetError, AssetName,
    AssetParseError, AssetType, DataViewList,
};

#[derive(Debug, Default)]
#[binrw::binrw]
#[br(little)]
#[bw(little)]
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
#[binrw::binrw]
#[br(little)]
#[bw(little)]
pub struct DataView {
    pub(crate) offset: u32,
    pub(crate) size: u32,
}

impl DataView {
    #[deprecated(note = "use binrw to parse")]
    pub fn from_reader<R: Read>(reader: &mut R) -> Result<DataView, std::io::Error> {
        let offset = reader.read_u32::<LittleEndian>()?;
        let size = reader.read_u32::<LittleEndian>()?;

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

#[derive(Debug, Clone)]
#[binrw::binrw]
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
    pub fn new(name: &str, asset_type: AssetType, unk_1: u32, unk_2: u32) -> Self {
        let mut name_bytes: AssetName = [0x00; 128];

        let bytes: Vec<u8> = name.bytes().take(128).collect();

        name_bytes[0..bytes.len()].copy_from_slice(&bytes);

        Self {
            name: name_bytes,
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

    #[deprecated(note = "use binrw to parse this")]
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AssetParseError> {
        if bytes.len() < size_of::<AssetMetadata>() {
            return Err(AssetParseError::InputTooSmall);
        }

        if bytes.len() > size_of::<AssetMetadata>() {
            println!(
                "Warning: parsing AssetMetadata from slice of size {}, but an AssetMetadata struct is only {} bytes in size. there may be a logic error in the program, and this should be checked.",
                bytes.len(),
                size_of::<AssetMetadata>()
            );
        }

        let mut cur = std::io::Cursor::new(bytes);

        let mut name: AssetName = [0u8; 128];
        cur.read_exact(&mut name)?;

        let asset_type_raw = cur.read_u32::<LittleEndian>()?;
        let asset_type: AssetType = asset_type_raw.try_into().map_err(|_| {
            AssetParseError::InvalidDataViews(format!("Invalid asset type: {}", asset_type_raw))
        })?;

        Ok(Self {
            name,
            asset_type,
            unk_1: cur.read_u32::<LittleEndian>()?,
            unk_2: cur.read_u32::<LittleEndian>()?,
        })
    }

    #[deprecated(note = "use binrw to serialize this")]
    pub fn to_bytes(&self) -> Vec<u8> {
        let Self {
            name,
            asset_type,
            unk_1,
            unk_2,
        } = self;
        let mut v = vec![0u8; 0x80];
        v[0..0x80].copy_from_slice(name);

        v.write_u32::<LittleEndian>((*asset_type).into())
            .expect("Failed to write to buffer");
        v.write_u32::<LittleEndian>(*unk_1)
            .expect("Failed to write to buffer");
        v.write_u32::<LittleEndian>(*unk_2)
            .expect("Failed to write to buffer");

        v
    }
}

pub type RawAsset = Asset<RawAssetData>;
impl RawAsset {
    pub fn from_dir<P: AsRef<path::Path>>(path: P) -> Result<Self, crate::Error> {
        let path_ref = path.as_ref();

        let contents: Vec<PathBuf> = std::fs::read_dir(path_ref)?
            .filter_map(|v| v.ok())
            .map(|v| v.path())
            .collect();

        let descriptor_path = contents
            .iter()
            .find(|p| {
                if let Some(file_name) = p.file_name() {
                    file_name == "descriptor"
                } else {
                    false
                }
            })
            .ok_or(AssetParseError::FileNotFound("descriptor".to_string()))?;

        let metadata_path = contents
            .iter()
            .find(|p| {
                if let Some(file_name) = p.file_name() {
                    file_name == "metadata"
                } else {
                    false
                }
            })
            .ok_or(AssetParseError::FileNotFound("metadata".to_string()))?;

        let resource_paths = contents.iter().filter(|p| {
            if let Some(file_name) = p.file_name() {
                file_name.to_str().unwrap().starts_with("resource")
            } else {
                false
            }
        });

        let metadata_bytes = std::fs::read(metadata_path)?;
        let descriptor_bytes = std::fs::read(descriptor_path)?;

        let resource_chunks: Vec<Vec<u8>> = resource_paths
            .into_iter()
            .map(std::fs::read)
            .collect::<Result<_, _>>()?;

        let metadata = std::io::Cursor::new(metadata_bytes.as_slice()).read_le()?;

        Ok(Self {
            metadata,
            data: RawAssetData {
                descriptor_bytes,
                resource_chunks,
            },
        })
    }
}

impl AssetData for RawAssetData {
    const ASSET_TYPE: AssetType = AssetType::Raw;
}

#[derive(Debug, Clone)]
pub struct RawAssetData {
    pub descriptor_bytes: Vec<u8>,
    pub resource_chunks: Vec<Vec<u8>>,
}

impl TryFrom<RawAssetData> for crate::xsb::XSoundbank {
    type Error = crate::Error;

    fn try_from(value: RawAssetData) -> Result<Self, Self::Error> {
        Ok(std::io::Cursor::new(&value.descriptor_bytes).read_le()?)
    }
}

impl TryFrom<crate::xsb::XSoundbank> for RawAssetData {
    type Error = crate::Error;

    fn try_from(_: crate::xsb::XSoundbank) -> Result<Self, Self::Error> {
        todo!("XSoundbank into asset not implemented yet")
    }
}

impl RawAssetData {
    pub fn new(descriptor_bytes: Vec<u8>, resource_chunks: Vec<Vec<u8>>) -> Self {
        Self {
            descriptor_bytes,
            resource_chunks,
        }
    }

    /// Combines the resource chunks into a single Vec and returns them
    pub fn resource(&self) -> Option<Vec<u8>> {
        (!self.resource_chunks.is_empty())
            .then(|| self.resource_chunks.clone().into_iter().flatten().collect())
    }

    #[deprecated(note = "Access the field directly instead")]
    pub fn descriptor_bytes(&self) -> &[u8] {
        &self.descriptor_bytes
    }
    #[deprecated(note = "Access the field directly instead")]
    pub fn descriptor_bytes_mut(&mut self) -> &mut Vec<u8> {
        &mut self.descriptor_bytes
    }

    pub fn resource_chunks(&self) -> Option<&Vec<Vec<u8>>> {
        (!self.resource_chunks.is_empty()).then_some(&self.resource_chunks)
    }
    pub fn resource_chunks_mut(&mut self) -> Option<&mut Vec<Vec<u8>>> {
        self.resource_chunks
            .is_empty()
            .then(|| self.resource_chunks.as_mut())
    }
}

#[derive(Debug, Default)]
pub struct BNLFile {
    header: BNLHeader,
    assets: Vec<RawAsset>,
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
    let bytes = std::fs::read(&path).expect("Unable to read BNL.");

    let bnl = BNLFile::from_bytes(&bytes).expect("Unable to parse BNL.");
    ```
    */
    pub fn from_bytes(bnl_bytes: &[u8]) -> Result<Self, crate::Error> {
        if bnl_bytes.len() < 40 {
            return Err(format!(
                "Length of BNL file must be at least 40 bytes (received {})",
                bnl_bytes.len()
            )
            .into());
        }

        let mut bytes = bnl_bytes[..40].to_vec();

        let mut cur = std::io::Cursor::new(bnl_bytes);

        let header = cur.read_le()?;

        let decompressed_bytes = miniz_oxide::inflate::decompress_to_vec_zlib(&bnl_bytes[40..])
            .map_err(|e| e.to_string())?;
        bytes.extend_from_slice(&decompressed_bytes);

        cur = std::io::Cursor::new(&bytes);

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
            let description: AssetDescription = cur.read_le()?;

            let desc_start: usize = description.descriptor_ptr as usize;
            let desc_end: usize = desc_start + description.descriptor_size as usize;
            let descriptor_bytes = descriptor_bytes
                .get(desc_start..desc_end)
                .ok_or_else(|| "index out of range".to_owned())?
                .to_vec();

            let resource_chunks = if description.resource_size == 0 {
                vec![]
            } else {
                DataViewList::from_bytes(
                    buffer_views_bytes
                        .get(description.dataview_list_ptr as usize..)
                        .ok_or_else(|| "bad data view list".to_owned())?,
                )?
                .slices(&buffer_bytes)?
                .iter()
                .map(|slice| slice.to_vec())
                .collect()
            };

            // TODO: Resize this then push into it
            new_bnl.assets.push(Asset {
                metadata: description.metadata,
                data: RawAssetData {
                    descriptor_bytes,
                    resource_chunks,
                },
            });
        }

        Ok(new_bnl)
    }

    pub fn to_bytes(&mut self) -> Result<Vec<u8>, crate::Error> {
        let mut asset_desc_section: Vec<u8> =
            vec![0x00; ASSET_DESCRIPTION_SIZE * self.assets.len()];
        let mut asset_desc_cur = std::io::Cursor::new(&mut asset_desc_section);

        let mut buffer_views_section: Vec<u8> = vec![];
        let mut buffer_section: Vec<u8> = vec![];
        let mut descriptors_section: Vec<u8> = vec![];

        self.assets.sort_by_key(|v| v.metadata.name().to_string());

        for asset in &self.assets {
            let mut asset_desc: AssetDescription = asset.metadata.clone().into();

            let num_chunks = asset.data.resource_chunks.len();
            if num_chunks > 0 {
                let dvl = DataViewList {
                    size: (8 + 8 * num_chunks) as u32,
                    num_views: num_chunks as u32,
                    views: asset
                        .data
                        .resource_chunks
                        .iter()
                        .map(|chunk| {
                            let offset = buffer_section.len();

                            // TODO: Find a way to propagate this, or safely ignore it
                            buffer_section.write_all(chunk)?;

                            Ok(DataView {
                                offset: offset as u32,
                                size: chunk.len() as u32,
                            })
                        })
                        .collect::<Result<_, crate::Error>>()?,
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
            asset_desc.descriptor_size = asset.data.descriptor_bytes.len() as u32;
            descriptors_section.extend_from_slice(&asset.data.descriptor_bytes);

            asset_desc_cur.write_le(&asset_desc)?;
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

        let mut writer = std::io::Cursor::new(&mut bytes);
        self.header.write_le(&mut writer)?;
        bytes[40..].copy_from_slice(&compressed_bytes);

        Ok(bytes)
    }

    /// Retrieves an asset by name and type, converting it to the target format if it matches the
    /// format of the asset's descriptor.
    ///
    /// # Errors
    /// - Name not found
    /// - Type mismatch
    /// - Parse error
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
    pub fn get_asset<AD>(&self, name: &str) -> Result<Asset<AD>, crate::Error>
    where
        AD: AssetData + TryFrom<RawAssetData, Error = crate::Error>,
    {
        let raw_asset = self
            .get_raw_asset(name)
            .ok_or_else(|| "not found".to_owned())?;

        let metadata = raw_asset.metadata.clone();

        if metadata.asset_type() != AD::asset_type() {
            return Err("type mismatch".into());
        }

        Ok(Asset {
            metadata: raw_asset.metadata.clone(),
            data: raw_asset.data.clone().try_into()?,
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
    pub fn get_assets<AD: AssetData + TryInto<RawAssetData, Error = crate::Error>>(
        &self,
    ) -> Vec<Asset<AD>> {
        let mut assets = Vec::new();

        for asset in &self.assets {
            let asset_desc = &asset.metadata;

            if asset_desc.asset_type() != AD::asset_type() {
                continue;
            }

            let Ok(transformed) = asset.data.clone().try_into() else {
                continue;
            };

            assets.push(Asset {
                metadata: asset.metadata.clone(),
                data: transformed,
            });
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

    pub fn modify_asset<AD, F>(&mut self, name: &str, f: F) -> Result<(), crate::Error>
    where
        AD: TryFrom<RawAssetData, Error = crate::Error>
            + TryInto<RawAssetData, Error = crate::Error>,
        F: FnOnce(&mut Asset<AD>) -> Result<(), crate::Error>,
    {
        let raw_asset = self.get_raw_asset_mut(name).ok_or("not found")?;
        let asset = raw_asset.clone();

        let mut asset = Asset {
            metadata: asset.metadata,
            data: asset.data.try_into()?,
        };

        f(&mut asset)?;

        *raw_asset = asset.to_raw_asset()?;

        Ok(())
    }

    pub fn remove_asset(&mut self, name: &str) -> Result<RawAsset, AssetError> {
        let mut index: Option<usize> = None;

        for (i, asset) in self.assets.iter().enumerate() {
            if asset.metadata.name() == name {
                index = Some(i);
                break;
            }
        }

        if let Some(ind) = index {
            return Ok(self.assets.remove(ind));
        }

        Err(AssetError::NotFound)
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

    pub fn append_asset<AD>(&mut self, asset: Asset<AD>) -> Result<(), crate::Error>
    where
        AD: AssetData + TryInto<RawAssetData, Error = crate::Error>,
    {
        self.append_raw_asset(asset.to_raw_asset()?);

        Ok(())
    }

    pub fn append_raw_asset(&mut self, new_raw_asset: RawAsset) {
        self.assets.push(new_raw_asset);
    }

    /// Inserts a RawAsset into a BNLFile, replacing it if it already exists.
    pub fn upsert_raw_asset(&mut self, new_raw_asset: RawAsset) {
        if let Some(asset) = self
            .assets
            .iter_mut()
            .find(|asset| asset.metadata.name() == new_raw_asset.metadata.name())
        {
            *asset = new_raw_asset;
        } else {
            self.assets.push(new_raw_asset);
        }
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

impl std::fmt::Display for BNLError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                BNLError::DecompressionFailure => "Decompression failure".to_owned(),
                BNLError::DataReadError(e) => format!("Data read error: {e}"),
            }
        )
    }
}

pub fn get_asset_names_list<P: AsRef<Path>>(path: P) -> Result<Vec<String>, crate::Error> {
    let file = std::fs::File::open(path.as_ref())?;

    let mut reader = BufReader::new(file);

    let header: BNLHeader = reader.read_le()?;

    let mut end_bytes = vec![0u8; header.asset_desc_loc.size as usize];
    reader.read_exact(&mut end_bytes)?;

    let decompressed_bytes = match miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(
        &end_bytes,
        size_of::<AssetDescription>() * header.file_count as usize,
    ) {
        Ok(v) => v,
        Err(e) => match e.status {
            // Too much input is ok
            TINFLStatus::HasMoreOutput => e.output,
            TINFLStatus::FailedCannotMakeProgress
            | TINFLStatus::BadParam
            | TINFLStatus::Adler32Mismatch
            | TINFLStatus::Failed
            | TINFLStatus::Done
            | TINFLStatus::NeedsMoreInput => {
                return Err("failed to decompress bnl file: needs more input".into());
            }
        },
    };

    decompressed_bytes
        .chunks_exact(size_of::<AssetDescription>())
        .map(|chunk| -> Result<String, crate::Error> {
            let mut string_bytes = Vec::new();
            chunk
                .take(size_of::<AssetName>() as u64)
                .read_until(0x00, &mut string_bytes)?;

            // Pop null terminator
            string_bytes.pop();

            Ok(String::from_utf8(string_bytes)?)
        })
        .collect()
}

pub fn get_aid_list(compressed_bnl: &[u8]) -> Result<Vec<String>, crate::Error> {
    let mut cur = std::io::Cursor::new(compressed_bnl);

    let header: BNLHeader = cur.read_le()?;

    let asset_descriptions = match miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(
        &compressed_bnl[40..],
        header.asset_desc_loc.size as usize,
    ) {
        Ok(v) => v,
        Err(miniz_oxide::inflate::DecompressError { status, output }) => match status {
            TINFLStatus::HasMoreOutput => output,
            _ => return Err("failed to decompress".into()),
        },
    };

    Ok(asset_descriptions
        .chunks_exact(size_of::<AssetDescription>())
        .filter_map(|chunk| {
            let mut string_bytes = vec![];

            chunk
                .take(size_of::<AssetName>() as u64)
                .read_until(0x00, &mut string_bytes)
                .ok()?;

            string_bytes.pop();

            String::from_utf8(string_bytes).ok()
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_bnl_from_raw() -> Result<(), crate::Error> {
        let tex_descriptor = include_bytes!("asset/test_data/texture0_descriptor").to_vec();
        let tex_image_bytes = include_bytes!("asset/test_data/texture0_resource0").to_vec();

        let metadata = AssetMetadata::new("aid_sometexture", AssetType::Texture, 0, 0);
        let raw_asset = RawAsset {
            metadata,
            data: RawAssetData::new(tex_descriptor, vec![tex_image_bytes]),
        };

        let mut new_bnl = BNLFile::default();
        new_bnl.append_raw_asset(raw_asset);

        let serialised = new_bnl.to_bytes()?;
        let deserialised = BNLFile::from_bytes(&serialised)
            .map_err(|_| "Failed to deserialise the BNL file which was just created in memory.")?;

        assert!(
            deserialised.assets.len() == 1,
            "The number of assets in the deserialised file is {} (expected 1)",
            deserialised.assets.len()
        );

        assert!(
            deserialised.get_raw_asset("aid_sometexture").is_some(),
            "No asset exists in the new bnl file with the name aid_sometexture"
        );

        Ok(())
    }
}
