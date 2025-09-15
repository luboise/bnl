use indexmap::IndexMap;

pub(crate) type ParamsShape = IndexMap<String, ParamDescriptor>;

pub trait HasParams {
    fn get_shape(&self) -> ParamsShape;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KnownUnknown<K, U: Clone>
where
    K: TryFrom<U>,
    U: Clone + From<K>,
{
    Known(K),
    Unknown(U),
}

impl<K, U> HasParams for KnownUnknown<K, U>
where
    K: HasParams + TryFrom<U>,
    U: Clone + From<K>,
{
    fn get_shape(&self) -> ParamsShape {
        match self {
            Self::Known(known_val) => known_val.get_shape(),
            Self::Unknown(_) => {
                IndexMap::new() // Return an empty hashmap to indicate no shape
            }
        }
    }
}

impl<K, U> From<U> for KnownUnknown<K, U>
where
    K: HasParams + TryFrom<U>,
    U: Clone + From<K>,
{
    fn from(value: U) -> Self {
        match value.clone().try_into() {
            Ok(known) => Self::Known(known),
            Err(_) => Self::Unknown(value),
        }
    }
}

pub trait Param {
    fn to_param_bytes(&self) -> Vec<u8>;
}

macro_rules! impl_param {
    ($t:ty) => {
        impl Param for $t {
            fn to_param_bytes(&self) -> Vec<u8> {
                self.to_le_bytes().to_vec()
            }
        }
    };
}

impl_param!(f32);
impl_param!(f64);
impl_param!(u8);
impl_param!(i8);
impl_param!(u16);
impl_param!(i16);
impl_param!(u32);
impl_param!(i32);
impl_param!(u64);
impl_param!(i64);

impl Param for Vec<u8> {
    fn to_param_bytes(&self) -> Vec<u8> {
        self.clone()
    }
}

impl Param for String {
    fn to_param_bytes(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

#[derive(Debug)]
pub enum ParamType {
    F32,
    F64,
    U8,
    I8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,

    String(usize),
    WString(usize),
    Bytes(usize),
}

impl ParamType {
    pub fn size(&self) -> usize {
        match self {
            Self::F32 | Self::U32 | Self::I32 => 4,
            Self::F64 | Self::U64 | Self::I64 => 8,
            Self::U8 | Self::I8 => 1,
            Self::U16 | Self::I16 => 2,

            Self::String(size) => *size,
            Self::WString(size) => *size,
            Self::Bytes(size) => *size,
        }
    }
}

#[derive(Debug)]
pub struct ParamDescriptor {
    pub(crate) param_type: ParamType,
    pub(crate) description: String,
}

impl ParamDescriptor {
    pub fn param_type(&self) -> &ParamType {
        &self.param_type
    }

    pub fn description(&self) -> &str {
        &self.description
    }
}
