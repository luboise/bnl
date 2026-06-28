use std::io::SeekFrom;

#[binrw::binrw]
#[derive(Clone)]
pub struct VertexShader {
    idk1: u16,
    num_dwords: u16,

    #[br(count = 4 * num_dwords * 4)]
    data: Vec<u8>,
}

impl VertexShader {
    pub fn byte_len(&self) -> usize {
        4 + self.data.len()
    }
}

impl std::fmt::Debug for VertexShader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { idk1, num_dwords, data } = self;

        f
            .debug_struct("VertexShader")
            .field("idk1", idk1)
            .field("num_dwords", num_dwords)
            .field("data len", &data.len())
            .finish()
    }
}


#[binrw::binrw]
#[br(little)]
#[bw(little)]
#[derive(Clone)]
pub struct OtherStream {
    #[br(
        assert(!stream.is_empty() && *stream.last().unwrap() == 0xffffffff),
        parse_with = binrw::helpers::until(|byte| *byte == 0xffffffff),
    )]
    stream: Vec<u32>,
}

impl OtherStream {
    pub fn byte_len(&self) -> usize {
        self.stream.len() * 4
    }
}

impl std::fmt::Debug for OtherStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { stream } = self;

        f.debug_struct("OtherStream")
            .field("stream length", &stream.len())
            .finish()
    }
}


#[binrw::binrw]
#[bw(stream = w)]
#[derive(Debug, Clone)]
#[expect(clippy::manual_non_exhaustive)]
pub struct NdVertexShaderData {
    #[br(temp)]
    #[bw(try_calc = u32::try_from(w.stream_position()?
        + (0x58)
        + u64::try_from(other_stream.byte_len() + vertex_shader.byte_len()).map_err(super::br_error(w))?))]
    stream_source_indices_ptr: u32,
    #[br(temp)]
    #[bw(try_calc = u32::try_from(stream_source_indices.len()))]
    num_stream_sources: u32,
    pub constant1: u32,
    pub constant2: u16,
   pub idk1: u16,
    // 0x10
   pub idk2: u32,
 pub   idk3: u32,
 pub   idk4: u32,
 pub   count1: u32,
    // 0x20
 pub   count2: u32,
 pub   count3: u32,
 pub   idk_2_1: u32,
 pub   idk_2_2: u32,
    // 0x30
    #[br(temp)]
    #[bw(try_calc = u32::try_from(w.stream_position()?
        + (0x58 - 0x30)))]
    other_stream_ptr: u32,
    #[br(temp)]
    #[bw(try_calc = u32::try_from(w.stream_position()?
        + (0x58 - 0x34)
        + u64::try_from(other_stream.byte_len()).map_err(super::br_error(w))?
        ))]
    vertex_shader_ptr: u32,
    #[br(temp)]
    #[bw(try_calc = u32::try_from(w.stream_position()?
        + (0x58 - 0x38)
        + u64::try_from(
            vertex_shader.byte_len() 
             + other_stream.byte_len() 
             + stream_source_indices.len() * 4
            ).map_err(super::br_error(w))?))]
    strides_ptr: u32,
    #[brw(magic = b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00ndVertexShader\x00\x00")]
    _name: (),
    #[br(seek_before = SeekFrom::Start(other_stream_ptr.into()))]
 pub   other_stream: OtherStream,
    #[br(seek_before=SeekFrom::Start(vertex_shader_ptr.into()))]
 pub   vertex_shader: VertexShader,
    #[br(count = num_stream_sources,
        seek_before = SeekFrom::Start(stream_source_indices_ptr.into()))]
 pub   stream_source_indices: Vec<u32>,
    #[br(count = num_stream_sources,
        seek_before = SeekFrom::Start(strides_ptr.into()))]
 pub   strides: Vec<u8>,
}

#[binrw::binrw]
#[bw(stream = w)]
#[derive(Debug, Clone)]
pub struct PixelShader {
    pixel_shader_type: u32, // used at runtime, expect 0?
    #[br(assert(shader_ptr != 0))]
    #[bw(try_calc = w.stream_position().ok().and_then(|v|u32::try_from(v).ok()).ok_or("bad conversion")
     .map(|pos| pos + 16))]
    shader_ptr: u32,
    length: u32,
    shader_handle: u32, // used at runtime for handle
    some_u32: u32,      // possibly padding?
    #[br(count = length)]
    shader: Vec<u8>,
}

impl PixelShader {
    pub fn size(&self) -> usize {
        5 * size_of::<u32>() + self.shader.len()
    }
}

#[binrw::binrw]
#[bw(stream = w)]
#[derive(Debug, Clone)]
pub struct NdShader2Data {
    #[br(temp)]
    #[bw(try_calc = u32::try_from(w.stream_position().unwrap()).map(|v| v + 8))]
    pixel_shader_ptr: u32,
    #[br(temp)]
    #[bw(calc = pixel_shader_sub.as_ref()
        .map(|shader2| pixel_shader_ptr + pixel_shader.size() as u32)
        .unwrap_or(0u32))]
    pixel_shader_ptr_2: u32,
    pixel_shader: PixelShader,
    #[br(if(pixel_shader_ptr_2 != 0), 
        seek_before = SeekFrom::Start(pixel_shader_ptr_2.into()))]
    pixel_shader_sub: Option<PixelShader>,
    #[brw(magic = b"ndShader2\x00\x00\x00")]
    _magic: (),
}

impl NdShader2Data {
    pub fn name_offset(&self) -> i64 {
        8 + self.pixel_shader.size() as i64
    }
}

