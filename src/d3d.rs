use num_enum::{IntoPrimitive, TryFromPrimitive};

type BitCount = usize;

pub trait PixelBits {
    fn bits_per_pixel(&self) -> BitCount;
}

#[derive(Debug, Clone, Copy, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u32)]
pub enum LinearColour {
    A1R5G5B5 = 0x00000010,
    A4R4G4B4 = 0x0000001D,
    A8 = 0x0000001F,
    A8B8G8R8 = 0x0000003F,
    A8R8G8B8 = 0x00000012,
    B8G8R8A8 = 0x00000040,
    G8B8 = 0x00000017,
    R4G4B4A4 = 0x0000003E,
    R5G5B5A1 = 0x0000003D,
    R5G6B5 = 0x00000011,
    R6G5B5 = 0x00000037,
    R8B8 = 0x00000016,
    R8G8B8A8 = 0x00000041,
    X1R5G5B5 = 0x0000001C,
    X8R8G8B8 = 0x0000001E,
}

impl PixelBits for LinearColour {
    fn bits_per_pixel(&self) -> BitCount {
        match self {
            // 8 bits
            LinearColour::A8 => 8,

            // 16 bits
            LinearColour::G8B8
            | LinearColour::R8B8
            | LinearColour::R5G6B5
            | LinearColour::R6G5B5
            | LinearColour::A1R5G5B5
            | LinearColour::R4G4B4A4
            | LinearColour::R5G5B5A1
            | LinearColour::X1R5G5B5
            | LinearColour::A4R4G4B4 => 16,

            // 32 bits
            LinearColour::A8B8G8R8
            | LinearColour::A8R8G8B8
            | LinearColour::B8G8R8A8
            | LinearColour::R8G8B8A8
            | LinearColour::X8R8G8B8 => 32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u32)]
pub enum LinearLuminance {
    A8L8 = 0x00000020,
    AL8 = 0x0000001B,
    L16 = 0x00000035,
    L8 = 0x00000013,

    V16U16 = 0x00000036,
    V8U8 = 0x00000017,
    L6V5U5 = 0x00000037,
    X8L8V8U8 = 0x0000001E,
    Q8W8V8U8 = 0x00000012,

    D24S8 = 0x0000002E,
    F24S8 = 0x0000002F,
    D16 = 0x00000030,
    F16 = 0x00000031,
}

impl PixelBits for LinearLuminance {
    fn bits_per_pixel(&self) -> BitCount {
        match self {
            // 8 bits
            LinearLuminance::L8 => 8,

            // 16 bits
            LinearLuminance::A8L8
            | LinearLuminance::AL8
            | LinearLuminance::L16
            | LinearLuminance::V8U8
            | LinearLuminance::D16
            | LinearLuminance::F16
            | LinearLuminance::L6V5U5 => 16,

            // 32 bits
            LinearLuminance::V16U16
            | LinearLuminance::X8L8V8U8
            | LinearLuminance::Q8W8V8U8
            | LinearLuminance::D24S8
            | LinearLuminance::F24S8 => 32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u32)]
pub enum Swizzled {
    /* Swizzled formats */
    A8R8G8B8 = 0x00000006,
    X8R8G8B8 = 0x00000007,
    R5G6B5 = 0x00000005,
    R6G5B5 = 0x00000027,
    X1R5G5B5 = 0x00000003,
    A1R5G5B5 = 0x00000002,
    A4R4G4B4 = 0x00000004,
    A8 = 0x00000019,
    A8B8G8R8 = 0x0000003A,
    B8G8R8A8 = 0x0000003B,
    R4G4B4A4 = 0x00000039,
    R5G5B5A1 = 0x00000038,
    R8G8B8A8 = 0x0000003C,
    R8B8 = 0x00000029,
    G8B8 = 0x00000028,
}

impl PixelBits for Swizzled {
    fn bits_per_pixel(&self) -> BitCount {
        match self {
            // 8 bits
            Swizzled::A8 => 8,

            // 16 bits
            Swizzled::R5G6B5
            | Swizzled::R6G5B5
            | Swizzled::X1R5G5B5
            | Swizzled::A1R5G5B5
            | Swizzled::A4R4G4B4
            | Swizzled::R4G4B4A4
            | Swizzled::R5G5B5A1
            | Swizzled::R8B8
            | Swizzled::G8B8 => 16,

            // 32 bits
            Swizzled::A8R8G8B8
            | Swizzled::X8R8G8B8
            | Swizzled::A8B8G8R8
            | Swizzled::B8G8R8A8
            | Swizzled::R8G8B8A8 => 32,
        }
    }
}

// TODO: Fix portability issue with enum
#[derive(Debug, Clone, Copy, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u32)]
pub enum StandardFormat {
    Unknown = 0xFFFFFFFF,

    P8 = 0x0000000B,

    L8 = 0x00000000,
    A8L8 = 0x0000001A,
    AL8 = 0x00000001,
    L16 = 0x00000032,

    V8U8 = 0x00000028,
    L6V5U5 = 0x00000027,
    X8L8V8U8 = 0x00000007,
    Q8W8V8U8 = 0x0000003A,
    V16U16 = 0x00000033,

    D16 = 0x0000002C,
    D24S8 = 0x0000002A,
    F16 = 0x0000002D,
    F24S8 = 0x0000002B,

    /* YUV formats */
    YUY2 = 0x00000024,
    UYVY = 0x00000025,

    /* Compressed formats */
    DXT1 = 0x0000000C,
    DXT2Or3 = 0x0000000E,
    DXT4Or5 = 0x0000000F,
}

impl PixelBits for StandardFormat {
    fn bits_per_pixel(&self) -> BitCount {
        match self {
            StandardFormat::Unknown => 0,

            // 4 bits
            StandardFormat::DXT1 => 4,

            // 8 bits
            StandardFormat::P8
            | StandardFormat::L8
            | StandardFormat::A8L8
            | StandardFormat::AL8
            | StandardFormat::DXT2Or3
            | StandardFormat::DXT4Or5 => 8,

            // 16 bits
            StandardFormat::L16
            | StandardFormat::V8U8
            | StandardFormat::L6V5U5
            | StandardFormat::D16
            | StandardFormat::F16
            | StandardFormat::YUY2
            | StandardFormat::UYVY => 16,

            // 32 bits
            StandardFormat::X8L8V8U8
            | StandardFormat::Q8W8V8U8
            | StandardFormat::V16U16
            | StandardFormat::D24S8
            | StandardFormat::F24S8 => 32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u32)]
pub enum D3DFormat {
    Swizzled(Swizzled),
    Luminance(LinearLuminance),
    Standard(StandardFormat),
    Linear(LinearColour),

    VertexData = 100,
    Index16 = 101,
    ForceDWORD = 0x7fffffff,
}

impl Into<u32> for D3DFormat {
    fn into(self) -> u32 {
        match self {
            D3DFormat::Swizzled(v) => v.into(),
            D3DFormat::Luminance(v) => v.into(),
            D3DFormat::Standard(v) => v.into(),
            D3DFormat::Linear(v) => v.into(),
            D3DFormat::VertexData => 100,
            D3DFormat::Index16 => 101,
            D3DFormat::ForceDWORD => 0x7fffffff,
        }
    }
}

impl PixelBits for D3DFormat {
    fn bits_per_pixel(&self) -> BitCount {
        match self {
            D3DFormat::Swizzled(format) => format.bits_per_pixel(),
            D3DFormat::Linear(format) => format.bits_per_pixel(),
            D3DFormat::Standard(format) => format.bits_per_pixel(),
            D3DFormat::Luminance(format) => format.bits_per_pixel(),
            D3DFormat::Index16 => 16, // 16 bits per index
            D3DFormat::VertexData => 0,
            D3DFormat::ForceDWORD => 0,
        }
    }
}

#[repr(u32)]
#[derive(Debug, Clone)]
pub enum D3DPrimitiveType {
    None = 0,
    PointList = 1,
    LineList = 2,
    LineLoop = 3,
    LineStrip = 4,
    TriangleList = 5,
    TriangleStrip = 6,
    TriangleFan = 7,
    QuadList = 8,
    QuadStrip = 9,
    Polygon = 10,
    Max = 11,
    Invalid = 0x7ffffff,
}

impl From<D3DPrimitiveType> for u32 {
    fn from(value: D3DPrimitiveType) -> Self {
        match value {
            D3DPrimitiveType::None => 0,
            D3DPrimitiveType::PointList => 1,
            D3DPrimitiveType::LineList => 2,
            D3DPrimitiveType::LineLoop => 3,
            D3DPrimitiveType::LineStrip => 4,
            D3DPrimitiveType::TriangleList => 5,
            D3DPrimitiveType::TriangleStrip => 6,
            D3DPrimitiveType::TriangleFan => 7,
            D3DPrimitiveType::QuadList => 8,
            D3DPrimitiveType::QuadStrip => 9,
            D3DPrimitiveType::Polygon => 10,
            D3DPrimitiveType::Max => 11,
            D3DPrimitiveType::Invalid => 0x7ffffff,
        }
    }
}

impl From<u32> for D3DPrimitiveType {
    fn from(value: u32) -> Self {
        match value {
            0 => Self::None,
            1 => Self::PointList,
            2 => Self::LineList,
            3 => Self::LineLoop,
            4 => Self::LineStrip,
            5 => Self::TriangleList,
            6 => Self::TriangleStrip,
            7 => Self::TriangleFan,
            8 => Self::QuadList,
            9 => Self::QuadStrip,
            10 => Self::Polygon,
            11 => Self::Max,
            0x7ffffff => Self::Invalid,
            _ => Self::Invalid,
        }
    }
}
