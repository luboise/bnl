use std::io::SeekFrom;

use binrw::BinReaderExt;

use super::prelude::*;
use crate::d3d::D3DPrimitiveType;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DrawCall {
    pub prim_type: D3DPrimitiveType,
    pub indices: Vec<u16>,
}

#[binrw::binrw]
#[derive(Debug, Clone, serde::Serialize)]
pub struct NdBGPushBufferData {
    push_buffer: NdPushBufferData,
    unknown_ptr_1: u32,
    unknown_ptr_2: u32,
    floats: [f32; 6],
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NdPushBufferData {
    pub(crate) num_draws: u32,
    pub(crate) unknown_u32_1: u32,
    pub(crate) unknown_u32_2: u32,
    pub(crate) unknown_u32_3: u32,
    pub(crate) prevent_culling_flag: u8,

    pub draw_calls: Vec<DrawCall>,
}

impl binrw::BinWrite for NdPushBufferData {
    type Args<'a> = ();

    fn write_options<W: std::io::prelude::Write + std::io::prelude::Seek>(
        &self,
        writer: &mut W,
        endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<()> {
        todo!()
    }
}

impl binrw::BinRead for NdPushBufferData {
    type Args<'a> = ();

    fn read_options<R: std::io::prelude::Read + std::io::prelude::Seek>(
        reader: &mut R,
        _: binrw::Endian,
        _: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<Self> {
        let num_draws = reader.read_u32::<LittleEndian>()?;
        let unknown_u32_1 = reader.read_u32::<LittleEndian>()?;
        let unknown_u32_2 = reader.read_u32::<LittleEndian>()?;
        let unknown_u32_3 = reader.read_u32::<LittleEndian>()?;

        let push_data_ptrs_ptr = reader.read_u32::<LittleEndian>()?;
        let primitive_types_ptr = reader.read_u32::<LittleEndian>()?;
        let vertex_counts_ptr = reader.read_u32::<LittleEndian>()?;
        let prevent_culling_flag = reader.read_u8()?;
        {
            let [pad1, pad2, pad3] = [reader.read_u8()?, reader.read_u8()?, reader.read_u8()?];

            if pad1 != 0 || pad2 != 0 || pad3 != 0 {
                return Err(binrw::Error::BadMagic {
                    pos: reader.stream_position().unwrap_or_default(),
                    found: Box::new([pad1, pad2, pad3]),
                });
            }
        }

        let mut data = Self {
            num_draws,
            unknown_u32_1,
            unknown_u32_2,
            unknown_u32_3,
            prevent_culling_flag,
            draw_calls: vec![],
        };

        let num_draws = usize::try_from(num_draws).map_err(|e| binrw::Error::Custom {
            pos: reader.stream_position().unwrap_or_default(),
            err: Box::new(e),
        })?;

        let draw_calls = {
            let push_data_ptrs = {
                reader.seek(SeekFrom::Start(push_data_ptrs_ptr.into()))?;
                (0..num_draws)
                    .map(|_| reader.read_u32::<LittleEndian>())
                    .collect::<Result<Vec<_>, _>>()?
            };

            let index_counts = {
                reader.seek(SeekFrom::Start(vertex_counts_ptr.into()))?;
                (0..num_draws)
                    .map(|_| reader.read_u32::<LittleEndian>())
                    .collect::<Result<Vec<_>, _>>()?
            };

            let primitive_types = {
                reader.seek(SeekFrom::Start(primitive_types_ptr.into()))?;
                (0..num_draws)
                    .map(|_| reader.read_le())
                    .collect::<Result<Vec<_>, _>>()?
            };

            let mut draw_calls = Vec::with_capacity(num_draws);

            for i in 0..num_draws {
                reader.seek(SeekFrom::Start(push_data_ptrs[i].into()));
                let num_indices = index_counts[i]
                    .try_into()
                    .map_err(|e| binrw::Error::Custom {
                        pos: reader.stream_position().unwrap_or_default(),
                        err: Box::new(e),
                    })?;

                let indices = (0..num_indices)
                    .map(|_| reader.read_u16::<LittleEndian>())
                    .collect::<Result<Vec<_>, _>>()?;

                draw_calls.push(DrawCall {
                    prim_type: primitive_types[i],
                    indices,
                });
            }

            draw_calls
        };

        data.draw_calls = draw_calls;

        Ok(data)
    }
}

impl NdPushBufferData {
    pub fn indices(&self) -> Vec<u16> {
        self.draw_calls
            .iter()
            .flat_map(|draw_call| draw_call.indices.clone())
            .collect()
    }
}
