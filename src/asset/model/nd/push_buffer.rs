use std::io::SeekFrom;

use binrw::{BinReaderExt, BinWriterExt};

use super::prelude::*;
use crate::{asset::model::nd::br_error, d3d::D3DPrimitiveType};

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
    pub(crate) unknown_u32_1: u32,
    pub(crate) unknown_u32_2: u32,
    pub(crate) unknown_u32_3: u32,
    pub(crate) prevent_culling_flag: u8,
    pub(crate) flag1: u8,
    pub(crate) flag2: u8,
    pub(crate) flag3: u8,

    pub draw_calls: Vec<DrawCall>,
}

impl NdPushBufferData {
    pub fn indices(&self) -> Vec<u16> {
        self.draw_calls
            .iter()
            .flat_map(|draw_call| draw_call.indices.clone())
            .collect()
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
        let flag1 = reader.read_u8()?;
        let flag2 = reader.read_u8()?;
        let flag3 = reader.read_u8()?;

        let mut data = Self {
            unknown_u32_1,
            unknown_u32_2,
            unknown_u32_3,
            prevent_culling_flag,
            flag1,
            flag2,
            flag3,
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
                reader.seek(SeekFrom::Start(push_data_ptrs[i].into()))?;
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

impl binrw::BinWrite for NdPushBufferData {
    type Args<'a> = ();

    fn write_options<W: std::io::prelude::Write + std::io::prelude::Seek>(
        &self,
        writer: &mut W,
        _endian: binrw::Endian,
        _args: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<()> {
        let NdPushBufferData {
            unknown_u32_1,
            unknown_u32_2,
            unknown_u32_3,
            prevent_culling_flag,
            flag1,
            flag2,
            flag3,
            draw_calls,
        } = self;

        let num_draws = u32::try_from(draw_calls.len()).map_err(br_error(writer))?;

        let vertex_counts = draw_calls
            .iter()
            .map(|draw_call| draw_call.indices.len().try_into())
            .collect::<Result<Vec<u32>, _>>()
            .map_err(br_error(writer))?;

        let primitive_types = draw_calls
            .iter()
            .map(|draw_call| u32::from(draw_call.prim_type))
            .collect::<Vec<_>>();

        writer.write_le(&num_draws)?;
        writer.write_le(&unknown_u32_1)?;
        writer.write_le(&unknown_u32_2)?;
        writer.write_le(&unknown_u32_3)?;

        // Vertex counts ptr
        let vertex_counts_ptr = (writer.stream_position()? + 0x14 + 0xc) as u32;
        let push_data_ptrs_ptr = vertex_counts_ptr + 4 * num_draws;
        let primitive_types_ptr = push_data_ptrs_ptr + 4 * num_draws;

        writer.write_le(&push_data_ptrs_ptr)?;
        writer.write_le(&primitive_types_ptr)?;
        writer.write_le(&vertex_counts_ptr)?;

        writer.write_le(&prevent_culling_flag)?;
        writer.write_le(&flag1)?;
        writer.write_le(&flag2)?;
        writer.write_le(&flag3)?;

        writer.write_all(b"ndPushBuffer\x00\x00\x00\x00")?;

        let (draw_ptrs, indices) = {
            let data_start = primitive_types_ptr + 4 * num_draws;

            let mut draw_ptrs = Vec::with_capacity(num_draws.try_into().map_err(br_error(writer))?);
            let mut indices =
                Vec::with_capacity(draw_calls.iter().map(|dc| dc.indices.len()).sum());

            for draw_call in draw_calls {
                let ptr = data_start + (indices.len() * size_of::<u16>()) as u32;
                draw_ptrs.push(ptr);
                indices.extend_from_slice(&draw_call.indices);
            }

            (draw_ptrs, indices)
        };

        writer.write_le(&vertex_counts)?;
        writer.write_le(&draw_ptrs)?;
        writer.write_le(&primitive_types)?;
        writer.write_le(&indices)?;

        Ok(())
    }
}
