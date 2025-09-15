pub mod ops;

use std::io::{Cursor, Read, Write};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::{
    VirtualResource,
    asset::{
        Asset, AssetDescriptor, AssetParseError,
        param::{HasParams, Param, ParamsShape},
        script::ops::{KnownOpcode, ScriptOpcode},
    },
    game::AssetType,
};

#[derive(Debug, Clone)]
pub struct ScriptDescriptor {
    operations: Vec<ScriptOperation>,
}

impl ScriptDescriptor {
    pub fn operations(&self) -> &[ScriptOperation] {
        &self.operations
    }

    pub fn operations_mut(&mut self) -> &mut Vec<ScriptOperation> {
        &mut self.operations
    }
}

#[derive(Debug, Clone)]
pub enum ScriptError {
    SizeMismatch,
    InvalidInput,
    UnsupportedOutputType,
}

#[derive(Debug)]
pub struct Script {
    name: String,
    descriptor: ScriptDescriptor,
    data: Vec<u8>,
}

impl Script {
    pub fn descriptor_mut(&mut self) -> &mut ScriptDescriptor {
        &mut self.descriptor
    }
}

#[derive(Debug, Clone)]
pub struct ScriptOperation {
    size: u32,
    opcode: ScriptOpcode,
    operand_bytes: Vec<u8>,
}

impl HasParams for ScriptOperation {
    fn get_shape(&self) -> ParamsShape {
        self.opcode.get_shape()
    }
}

impl ScriptOperation {
    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn opcode(&self) -> &ScriptOpcode {
        &self.opcode
    }

    pub fn operand_bytes(&self) -> &[u8] {
        &self.operand_bytes
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let size = self.operand_bytes.len() + 8;

        let mut bytes = vec![0x00; size];

        let mut cur = Cursor::new(&mut bytes[..]);

        cur.write_u32::<LittleEndian>(size as u32);
        cur.write_u32::<LittleEndian>(self.opcode.into());
        cur.write_all(self.operand_bytes());

        bytes
    }

    pub fn operand_bytes_mut(&mut self) -> &mut Vec<u8> {
        &mut self.operand_bytes
    }

    pub fn set_param_by_name<T: Param>(&mut self, name: &str, val: T) -> Result<(), ScriptError> {
        let shape = self.get_shape();
        if let Some(details) = shape.get(name) {
            if size_of::<T>() != details.param_type.size() {
                return Err(ScriptError::SizeMismatch);
            }

            let bytes = val.to_param_bytes();

            // TODO: Make this based on the parameter's actual offset
            let offset = 0;

            self.operand_bytes_mut()[offset..].copy_from_slice(&bytes);

            Ok(())
        } else {
            Err(ScriptError::UnsupportedOutputType)
        }
    }
}

impl AssetDescriptor for ScriptDescriptor {
    fn from_bytes(data: &[u8]) -> Result<Self, AssetParseError> {
        if data.len() < 8 {
            return Err(AssetParseError::InputTooSmall);
        }

        let mut cur = Cursor::new(data);

        let mut operations = Vec::new();

        let mut size = cur.read_u32::<LittleEndian>()?;
        let mut opcode = cur.read_u32::<LittleEndian>()?;

        while opcode != 0 {
            if size < 8 {
                return Err(AssetParseError::ErrorParsingDescriptor);
            }

            let mut operand_bytes = vec![0x00; (size as usize) - 8];
            cur.read_exact(&mut operand_bytes)?;

            operations.push(ScriptOperation {
                size,
                opcode: opcode.into(),
                operand_bytes,
            });

            size = cur.read_u32::<LittleEndian>()?;
            opcode = cur.read_u32::<LittleEndian>()?;
        }

        if size == 8 && opcode == 0 {
            operations.push(ScriptOperation {
                size: 8,
                opcode: ScriptOpcode::Known(KnownOpcode::EndScript),
                operand_bytes: [].to_vec(),
            });
        } else {
            // Size mismatch
            return Err(AssetParseError::ErrorParsingDescriptor);
        }

        // TODO: Sanity check the read length here
        Ok(ScriptDescriptor { operations })
    }

    fn to_bytes(&self) -> Result<Vec<u8>, AssetParseError> {
        let mut bytes = Vec::new();

        self.operations
            .iter()
            .map(|op| op.to_bytes())
            .for_each(|b| bytes.extend_from_slice(&b));

        Ok(bytes)
    }

    fn size(&self) -> usize {
        self.operations().iter().map(|v| v.size() as usize).sum()
    }

    fn asset_type() -> AssetType {
        AssetType::ResScript
    }
}

impl Asset for Script {
    type Descriptor = ScriptDescriptor;

    fn new(
        name: &str,
        descriptor: &Self::Descriptor,
        virtual_res: &VirtualResource,
    ) -> Result<Self, AssetParseError> {
        Ok(Script {
            name: name.to_string(),
            descriptor: descriptor.clone(),
            data: virtual_res.get_all_bytes(),
        })
    }

    fn descriptor(&self) -> &Self::Descriptor {
        &self.descriptor
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn resource_data(&self) -> Vec<u8> {
        self.data.clone()
    }
}
