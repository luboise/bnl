use binrw::{BinReaderExt, BinWriterExt};
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Read, Seek, SeekFrom, Write};

use crate::asset::model::nd::{ModelReadContext, ModelWriteContext, Nd, br_error};

#[derive(Clone, Debug)]
pub struct ModelSubresource {
    pub unknown1: u32,
    pub unknown2: u32,
    // nodes_ptr: u32,
    // num_nodes: u32,

    // key_values_ptr: u32,
    // map2_ptr: u32,
    pub floats: [f32; 4],
    pub next_ptr: u32,
    pub model_model_root: u32,
    pub nodes: Vec<Nd>,
    pub properties: indexmap::IndexMap<String, Vec<u8>>,
}

impl ModelSubresource {
    #[deprecated(note = "use BinReaderExt::read_le_args(ModelReadContext)")]
    pub fn from_bytes(
        bytes: &[u8],
        resource: &[u8],
        _resource_base: u32,
    ) -> Result<Self, crate::Error> {
        Ok(std::io::Cursor::new(bytes).read_le_args(ModelReadContext {
            properties: &Default::default(),
            resource,
        })?)
    }

    #[deprecated(note = "use ModelSubresource.nodes")]
    pub fn primitives(&self) -> &[Nd] {
        &self.nodes
    }
}

impl binrw::BinRead for ModelSubresource {
    type Args<'a> = ModelReadContext<'a>;

    fn read_options<R: Read + Seek>(
        reader: &mut R,
        _endian: binrw::Endian,
        mrc: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<Self> {
        let ModelReadContext {
            properties: _,
            resource,
        } = mrc;

        // FIXME: make this more efficient
        let mut reader = {
            // Model subres starts with 0x40, which points to 0x20 inside of the resource
            // => Skip 0x20 in and set that as zero
            let begin_ptr = reader.read_u32::<LittleEndian>()?;
            reader.seek(SeekFrom::Start(begin_ptr.into()))?;
            std::io::Cursor::new(reader.bytes().collect::<Result<Vec<_>, _>>()?)
        };

        let unknown1 = reader.read_u32::<LittleEndian>()?;
        let unknown2 = reader.read_u32::<LittleEndian>()?;
        let primitive_ptrs_start = reader.read_u32::<LittleEndian>()?;
        let primitive_count = reader.read_u32::<LittleEndian>()?;
        let properties_ptr = reader.read_u32::<LittleEndian>()?;
        let properties = {
            if properties_ptr == 0 {
                Default::default()
            } else {
                let mut reader = reader.clone();
                reader.seek(SeekFrom::Start(properties_ptr.into()))?;
                reader
                    .read_le::<ModelKeyValues>()?
                    .try_into()
                    .map_err(|e| binrw::Error::Custom {
                        pos: reader.stream_position().unwrap_or_default(),
                        err: Box::new(format!(
                            "unable to convert model_properties to hashmap: {e}"
                        )),
                    })?
            }
        };

        let map2_ptr = reader.read_u32::<LittleEndian>()?;
        if map2_ptr != 0 {
            return Err(binrw::Error::AssertFail {
                pos: reader.stream_position().unwrap_or(0),
                message: "map2_ptr is not 0".to_owned(),
            });
        }

        let floats = reader.read_le()?;
        let next_ptr = reader.read_le()?;
        let model_model_root = reader.read_le()?;

        let stream_end_position = reader.stream_position()?;

        let primitive_ptrs: Vec<u32> = {
            let mut reader = reader.clone();
            reader.seek(SeekFrom::Start(primitive_ptrs_start as u64))?;

            (0..primitive_count as usize)
                .map(|_| reader.read_u32::<LittleEndian>())
                .collect::<Result<_, _>>()?
        };

        let mrc = ModelReadContext::new(&properties, resource);

        let mut nodes = vec![];
        for primitive_ptr in primitive_ptrs {
            let mut reader = reader.clone();
            reader.seek(SeekFrom::Start(primitive_ptr.into()))?;
            let nd: Nd = reader.read_le_args((mrc,))?;
            nodes.push(nd);
        }

        reader.seek(SeekFrom::Start(stream_end_position))?;

        Ok(Self {
            unknown1,
            unknown2,
            floats,
            next_ptr,
            model_model_root,
            nodes,
            properties,
        })
    }
}

impl binrw::BinWrite for ModelSubresource {
    type Args<'a> = ModelWriteContext;

    fn write_options<W: std::io::prelude::Write + Seek>(
        &self,
        writer: &mut W,
        _endian: binrw::Endian,
        mwc: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<()> {
        let base = u32::try_from(writer.stream_position()?).map_err(br_error(writer))?;

        let subres = {
            let mut subres = vec![];
            let mut cur = std::io::Cursor::new(&mut subres);

            let ModelSubresource {
                unknown1,
                unknown2,
                floats,
                next_ptr,
                model_model_root,
                nodes,
                properties,
            } = self;

            cur.write_le(&unknown1)?;
            cur.write_le(&unknown2)?;
            // nodes ptr
            cur.write_le(&0x30u32)?;
            cur.write_le(&(self.nodes.len() as u32))?;
            // properties_ptr
            cur.write_le(&0u32)?;
            // map2
            cur.write_le(&0u32)?;

            cur.write_le(&floats)?;
            cur.write_le(&next_ptr)?;
            cur.write_le(&model_model_root)?;

            {
                let ptrs_start = cur.stream_position()? as u32;
                let mut nd_ptr = ptrs_start + 4 * nodes.len() as u32;

                for node in nodes {
                    nd_ptr = {
                        cur.write_le(&nd_ptr)?;
                        let restore = cur.stream_position()?;
                        cur.seek(SeekFrom::Start(nd_ptr.into()))?;
                        cur.write_le_args(node, mwc.clone())?;
                        let write_end = cur.stream_position()?;
                        cur.seek(SeekFrom::Start(restore))?;
                        write_end as u32
                    };
                }

                // Skip to after the nodes after writing them
                cur.seek(SeekFrom::Start(nd_ptr.into()))?;
            }

            for (nd_offset, indices) in std::mem::take(&mut mwc.borrow_mut().rigid_entries) {
                let ptr = cur.stream_position()? as u32;

                cur.seek(SeekFrom::Start(nd_offset))?;
                cur.write_le(&ptr)?;
                cur.seek(SeekFrom::Start(ptr.into()))?;
                cur.write_all(&indices)?;
            }

            let properties = &mwc.borrow().properties;

            if !properties.is_empty() {
                let properties_pos = cur.stream_position()?;
                cur.write_le(&ModelKeyValues::from(properties.clone()))?;

                let restore = cur.stream_position()?;

                // update the properties pos
                cur.seek(SeekFrom::Start(0x10))?;
                cur.write_le(&(properties_pos as u32))?;

                cur.seek(SeekFrom::Start(restore))?;
            }

            subres
        };

        let subres = (base + 0x20)
            .to_le_bytes()
            .into_iter()
            .chain([0u8; 32 - 4])
            .chain(subres)
            .collect::<Vec<_>>();

        writer.write_all(&subres)?;

        Ok(())
    }
}

#[binrw::binread]
#[br(little, stream = r)]
#[derive(Clone, Debug, Default)]
struct ModelKeyValues {
    #[br(temp)]
    num_key_values: u32,
    #[br(temp, assert(r.stream_position().is_ok_and(|v| v == u64::from(key_values_ptr))))]
    key_values_ptr: u32,
    #[br(count = num_key_values)]
    key_values: Vec<ModelKeyValue>,
}

impl From<indexmap::IndexMap<String, Vec<u8>>> for ModelKeyValues {
    fn from(value: indexmap::IndexMap<String, Vec<u8>>) -> Self {
        let key_values = value
            .into_iter()
            .map(|(k, v)| ModelKeyValue {
                key: k.into(),
                value: v,
            })
            .collect();

        Self { key_values }
    }
}

impl TryFrom<ModelKeyValues> for indexmap::IndexMap<String, Vec<u8>> {
    type Error = crate::Error;

    fn try_from(value: ModelKeyValues) -> Result<Self, Self::Error> {
        let mut hm = indexmap::IndexMap::new();

        for ModelKeyValue { key, value } in value.key_values {
            hm.insert(String::from_utf8(key.0)?, value);
        }

        Ok(hm)
    }
}

impl binrw::BinWrite for ModelKeyValues {
    type Args<'a> = ();

    fn write_options<W: std::io::prelude::Write + Seek>(
        &self,
        writer: &mut W,
        _: binrw::Endian,
        _: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<()> {
        writer.write_le(&(self.key_values.len() as u32))?;

        if self.key_values.is_empty() {
            writer.write_le(&0u32)?;
            return Ok(());
        } else {
            let entries_start = (writer.stream_position()? + 0x4) as u32;
            writer.write_le(&entries_start)?;
        }

        let mut data_bytes = vec![];

        let mut data_ptr = (self.key_values.len() as u64 * 12 + writer.stream_position()?) as u32;
        for entry in &self.key_values {
            let key_buf = {
                let mut buf = entry.key.0.clone();
                if buf.last() != Some(&0) {
                    buf.push(0);
                }
                while buf.len() % 4 != 0 {
                    buf.push(0xFD);
                }
                buf
            };

            // key_ptr
            writer.write_le(&data_ptr)?;
            data_ptr += key_buf.len() as u32;
            data_bytes.extend(key_buf);

            // value_ptr
            writer.write_le(&data_ptr)?;
            data_ptr += entry.value.len() as u32;
            data_bytes.extend_from_slice(&entry.value);

            // value_size
            writer.write_le(&(entry.value.len() as u32))?;
        }

        writer.write_le(&data_bytes)?;

        Ok(())
    }
}

#[binrw::binread]
#[br(little)]
#[derive(Clone, Debug, Default)]
struct ModelKeyValue {
    #[br(parse_with = binrw::FilePtr32::parse)]
    key: binrw::NullString,
    #[br(temp)]
    value_ptr: u32,
    #[br(temp)]
    value_size: u32,
    #[br(restore_position, count = value_size, seek_before = SeekFrom::Start(value_ptr as u64))]
    value: Vec<u8>,
}
