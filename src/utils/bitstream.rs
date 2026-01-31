pub type Error = Box<dyn std::error::Error>;

/*
impl<T> From<T> for Error
where
    T: Into<Box<dyn std::error::Error>>,
{
    fn from(value: T) -> Self {
        Self { err: value.into() }
    }
}
*/

pub struct BitStream<'a> {
    bit_cursor: usize,
    bytes: &'a [u8],
}

impl<'a> BitStream<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bit_cursor: 0,
            bytes,
        }
    }

    // pub fn read(&mut self) -> Result<u32, Error> {}

    pub fn len(&self) -> usize {
        self.bytes.len() * 8
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn get_u8_parts(
        &self,
        starting_byte: usize,
        offset: u32,
    ) -> Result<(Option<u8>, u8), Error> {
        // println!("Getting u8 parts at [{starting_byte}:{offset}]");
        if starting_byte >= self.bytes.len() {
            return Err(format!(
                "Unable to get byte {} for a keyframe of only {} bytes",
                starting_byte + 1,
                self.bytes.len()
            )
            .into());
        }

        // 0b00000111 for offset 5
        let low_mask = (2usize.pow(8 - offset) - 1) as u8;
        let (low_bits, _) = self.bytes[starting_byte].overflowing_shr(offset);
        let low_bits = low_bits & low_mask;

        if offset == 0 || starting_byte + 1 >= self.bytes.len() {
            return Ok((None, low_bits));
        }

        // 0b11111000 for offset 5
        let high_mask = !low_mask;
        let (high_bits, _) = self.bytes[starting_byte + 1].overflowing_shl(8 - offset);
        let high_bits = high_bits & high_mask;

        Ok((Some(high_bits & high_mask), low_bits & low_mask))
    }

    #[inline]
    pub fn get_u8(&self, starting_byte: usize, offset: u32) -> Result<u8, Error> {
        let (high, low) = self.get_u8_parts(starting_byte, offset)?;

        Ok(high.unwrap_or_default() | low)
    }

    pub fn get_bits(&self, mut first_bit: usize, num_bits: usize) -> Result<u32, Error> {
        let offset = first_bit % 8;

        let mut bits_read = 0;

        let mut ret = 0u32;

        while bits_read < num_bits {
            let num_read = (8 - offset).min(num_bits - bits_read);

            let byte = self.get_u8_parts(first_bit / 8, (first_bit % 8) as u32)?;

            let mask = 2u32.pow(num_read as u32) - 1;
            let byte = u32::from(byte.0.unwrap_or(0) | byte.1) & mask;

            ret |= byte << bits_read;

            bits_read += num_read;
            first_bit += num_read;
        }

        Ok(ret & (2u32.pow(num_bits as u32) - 1))
    }

    /// Read up to 32 bits from the bitstream
    pub fn read(&mut self, num_bits: usize) -> Result<u32, Error> {
        // println!(
        //     "Getting {num_bits} bits at [{}:{}]",
        //     self.bit_cursor / 8,
        //     self.bit_cursor % 8
        // );
        let ret = self.get_bits(self.bit_cursor, num_bits)?;
        self.bit_cursor += num_bits;
        Ok(ret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitreader_offset() -> Result<(), Error> {
        const BYTES: [u8; 4] = [0b11100000, 0b10101001, 0b00000010, 0b11111111];

        let stream = BitStream::new(&BYTES);

        let value = stream.get_u8(0, 5)?;

        assert_eq!(value, 0b01001111);

        Ok(())
    }

    #[test]
    fn bitreader_quaternion() -> Result<(), Error> {
        const BYTES: [u8; 0x08] = [
            0b10100010, 0b11100010, 0b11111010, 0b10011010, 0b11000110, 0b1000010, 0b10, 0b10,
        ];

        let stream = BitStream::new(&BYTES);

        assert_eq!(stream.get_bits(0, 11)?, 674);
        assert_eq!(stream.get_bits(11, 13)?, 8028);

        Ok(())
    }

    #[test]
    fn bitreader_read() -> Result<(), Error> {
        const BYTES: [u8; 0x08] = [
            0b10100010, 0b11100010, 0b11111010, 0b10011010, 0b11000110, 0b1000010, 0b10, 0b10,
        ];

        let mut stream = BitStream::new(&BYTES);

        assert_eq!(stream.read(11)?, 674);
        assert_eq!(stream.read(13)?, 8028);
        assert_eq!(stream.read(9)?, 154);

        Ok(())
    }
}
