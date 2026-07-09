use std::{
    fs,
    io::{self, SeekFrom},
    path::{Path, PathBuf},
};

use binrw::{BinReaderExt, BinWriterExt};
use serde::Deserialize;

pub mod soundbank;
pub use soundbank::XSoundbank;

pub type WavebankName = [u8; 16];
pub type SoundbankName = WavebankName;

pub fn dump_wav_entries(wav_files: &[WavEntry], dump_dir: PathBuf) -> Result<(), crate::Error> {
    let num_digits = (wav_files.len().checked_ilog10().unwrap_or(0) + 1) as usize;

    for (i, wav) in wav_files.iter().enumerate() {
        let out_path = dump_dir.join(format!("wavebank_{:0width$}.wav", i, width = num_digits));
        println!("Dumping to {}", out_path.display());
        wav.dump(out_path)?;

        let raw_out_path = dump_dir.join(format!("wavebank_raw_{}", i));
        wav.dump_raw(raw_out_path)?;
    }

    Ok(())
}

const XWAVEBANK_HEADER_SIZE: usize = 40;

#[binrw::binread]
#[derive(Debug, Deserialize)]
pub struct XWavebank {
    #[brw(magic = b"WBND")]
    _wbnd: (),

    pub version: u32,

    header_size: u32, // Size of a WavebankHeader
    wavebanks_ptr: u32,

    #[br(temp)]
    wav_entries_ptr: u32,
    #[br(temp)]
    wav_entries_size: u32, // Total size of all the wav entries in bytes
    //
    unknown_count_2: u32,

    unknown_1: u32,

    #[br(temp)]
    wave_data_ptr: u32,
    #[br(temp)]
    wave_data_length: u32,

    pub unknown_2: u32,
    pub unknown_3: u32,
    pub name: WavebankName,

    pub unknown_4: u32,
    pub unknown_5: u32,
    pub block_alignment: u32,

    #[br(args { inner: (wave_data_ptr,) }, restore_position,
        seek_before = SeekFrom::Start(wav_entries_ptr.into()),
        count = wav_entries_size / 24)]
    pub wav_entries: Vec<WavEntry>,

    #[br(restore_position, count = wave_data_length, seek_before = SeekFrom::Start(wave_data_ptr.into()))]
    pub wave_data: Vec<u8>,
}

impl binrw::BinWrite for XWavebank {
    type Args<'a> = ();

    fn write_options<W: io::prelude::Write + io::prelude::Seek>(
        &self,
        writer: &mut W,
        _: binrw::Endian,
        _: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<()> {
        let Self {
            _wbnd,
            version,
            header_size,
            wavebanks_ptr,
            wav_entries,
            unknown_count_2,
            unknown_1,
            unknown_2,
            unknown_3,
            name,
            unknown_4,
            unknown_5,
            block_alignment,
            wave_data,
        } = self;

        writer.write_all(b"WBND")?;
        writer.write_le(version)?;
        writer.write_le(header_size)?;
        writer.write_le(wavebanks_ptr)?;

        // 0x10

        // entries_ptr
        writer.write_le(&0x00000050u32)?;

        let wav_entries_len = (wav_entries.len() * 0x18) as u32;
        writer.write_le(&wav_entries_len)?;

        writer.write_le(unknown_count_2)?;
        writer.write_le(unknown_1)?;

        // 0x20

        let wav_data_start = (0x50 + wav_entries_len).next_multiple_of(*block_alignment);

        writer.write_le(&wav_data_start)?;
        writer.write_le(
            &((wav_entries
                .iter()
                .map(|entry| {
                    entry
                        .bytes
                        .len()
                        .next_multiple_of(*block_alignment as usize)
                })
                .sum::<usize>()) as u32),
        )?;

        writer.write_le(unknown_2)?;
        writer.write_le(unknown_3)?;

        // 0x30
        writer.write_le(name)?;
        // 0x40
        writer.write_le(unknown_4)?;
        writer.write_le(unknown_5)?;
        writer.write_le(block_alignment)?;
        // pad to 0x40
        writer.write_le(&0u32)?;

        let mut wave_ptr = 0u32;

        let align_writer = |writer: &mut W| -> Result<(), binrw::Error> {
            let block_align = *block_alignment as usize;

            let pos = writer.stream_position()?;
            if block_align > 0 && !pos.is_multiple_of(block_align as u64) {
                let diff = pos.next_multiple_of(block_align as u64) - pos;
                // print!("seeking forward from 0x{pos:x} + 0x{diff:x} = ");
                writer.write_all(&vec![0u8; diff as usize])?;
            }

            Ok(())
        };

        for entry in wav_entries {
            let WavEntry {
                unknown_1,
                format,
                wav_data_offset: _,
                num_bytes: _,
                unknown_2,
                unknown_3,
                bytes,
            } = entry;

            writer.write_le(unknown_1)?;
            writer.write_le(&u32::try_from(format.clone()).expect("failed to convert format"))?;

            let wave_size = bytes.len() as u32;
            writer.write_le(&wave_ptr)?;
            writer.write_le(&wave_size)?;
            wave_ptr += wave_size.next_multiple_of(*block_alignment);

            writer.write_le(unknown_2)?;
            writer.write_le(unknown_3)?;
        }

        align_writer(writer)?;

        for entry in wav_entries {
            writer.write_all(&entry.bytes)?;

            align_writer(writer)?;
        }

        Ok(())
    }
}

pub struct Wavebank {
    id: u32,
    num_entries: u32,
    name: [char; 16],
    idk1: u32,
    idk2: u32,
    num_or_ptr: u32,
    idk3: u32,
}

const RAW_WAV_ENTRY_SIZE: usize = 5 * size_of::<u32>() + 4 * size_of::<u8>();

/// Microsoft WAVEBANKMINIWAVEFORMAT
/// https://learn.microsoft.com/en-us/previous-versions/bb206350(v=vs.85)
#[derive(Debug, Clone)]
pub struct WaveBankMiniWaveFormat1 {
    /// false for uncompressed PCM, true for compressed
    pub is_compressed: bool,
    /// Number of audio channels
    pub num_channels: u8,
    /// 27 bit value representing number of samples per second
    pub samples_per_sec: u32,
    /// Indicates 16 byte samples when true
    pub uses_wide_format: bool,
}

impl From<u32> for WaveBankMiniWaveFormat1 {
    fn from(value: u32) -> Self {
        // DWORD wFormatTag : 1;
        let is_compressed: bool = value & 0x1 == 1;

        // DWORD nChannels : 3;
        let num_channels: u8 = (value >> 1) as u8 & 0b111;

        // DWORD nSamplesPerSec : 27;
        let samples_per_sec: u32 = (value >> (1 + 3)) & 0x1FFFFFF; // 27 bits

        // DWORD wBitsPerSample : 1;
        let uses_wide_format: bool = ((value >> (1 + 3 + 27)) & 1u32) == 1;

        Self {
            is_compressed,
            num_channels,
            samples_per_sec,
            uses_wide_format,
        }
    }
}

/// Wine WAVEBANKMINIWAVEFORMAT
/// https://source.winehq.org/source/include/xact3wb.h
#[derive(Debug, Clone, Deserialize)]
pub struct WaveBankMiniWaveFormat3 {
    /// DWORD wFormatTag : 2
    pub format_tag: u8,
    /// DWORD nChannels : 3;
    pub num_channels: u8,
    /// DWORD nSamplesPerSec : 18;
    pub samples_per_sec: u32,
    /// DWORD wBlockAlign    :  8;
    pub block_align: u8,
    /// DWORD wBitsPerSample :  1;
    pub uses_wide_format: bool,
}

impl From<u32> for WaveBankMiniWaveFormat3 {
    fn from(value: u32) -> Self {
        let format_tag: u8 = (value & 0b11) as u8;

        // DWORD nChannels : 3
        let num_channels: u8 = (value >> 2) as u8 & 0b111;

        // DWORD nSamplesPerSec : 18;
        let samples_per_sec: u32 = (value >> (2 + 3)) & 0x3FFFF; // 18 bits

        let block_align: u8 = (value >> (2 + 3 + 18)) as u8; // 8 bits

        // DWORD wBitsPerSample : 1;
        let uses_wide_format: bool = ((value >> (2 + 3 + 18 + 8)) & 1u32) == 1;

        Self {
            format_tag,
            num_channels,
            samples_per_sec,
            block_align,
            uses_wide_format,
        }
    }
}

impl TryFrom<WaveBankMiniWaveFormat3> for u32 {
    type Error = crate::Error;

    fn try_from(value: WaveBankMiniWaveFormat3) -> Result<Self, Self::Error> {
        let WaveBankMiniWaveFormat3 {
            format_tag,
            num_channels,
            samples_per_sec,
            block_align,
            uses_wide_format,
        } = value;

        let mut ret = 0u32;

        ret |= (format_tag as u32) & 0b11;
        ret |= (num_channels as u32 & 0b111) << 2;
        ret |= (samples_per_sec & 0x3ffff) << 5;
        ret |= (block_align as u32) << 23;
        ret |= (uses_wide_format as u32) << 31;

        Ok(ret)
    }
}

impl binrw::BinRead for WaveBankMiniWaveFormat3 {
    type Args<'a> = ();

    fn read_options<R: io::prelude::Read + io::prelude::Seek>(
        reader: &mut R,
        _: binrw::Endian,
        _: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<Self> {
        Ok(Self::from(reader.read_le::<u32>()?))
    }
}

impl binrw::BinWrite for WaveBankMiniWaveFormat3 {
    type Args<'a> = ();

    fn write_options<W: io::prelude::Write + io::prelude::Seek>(
        &self,
        writer: &mut W,
        _: binrw::Endian,
        _: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<()> {
        let pos = writer.stream_position().unwrap_or_default();
        writer.write_le(
            &u32::try_from(self.clone()).map_err(|_| binrw::Error::Custom {
                pos,
                err: Box::new("failed to serialize WaveBankMiniWaveFormat3".to_owned()),
            })?,
        )
    }
}

impl Default for WaveBankMiniWaveFormat3 {
    fn default() -> Self {
        Self {
            format_tag: 0,
            num_channels: 2,
            samples_per_sec: 44100,
            block_align: 200,
            uses_wide_format: true,
        }
    }
}

#[derive(Debug, Deserialize, Default, Clone)]
#[binrw::binread]
#[br(import(wav_data_start: u32), little)]
pub struct WavEntry {
    pub unknown_1: u32,

    pub format: WaveBankMiniWaveFormat3,

    // #[br(temp)]
    wav_data_offset: u32,
    // #[br(temp)]
    num_bytes: u32,
    pub unknown_2: u32,
    pub unknown_3: u32,

    #[br(count = num_bytes, restore_position, seek_before = SeekFrom::Start((wav_data_start + wav_data_offset).into()))]
    pub bytes: Vec<u8>,
}

impl WavEntry {
    pub fn dump<P: AsRef<Path>>(&self, out_path: P) -> Result<(), io::Error> {
        fs::create_dir_all(out_path.as_ref().parent().unwrap())?;

        let samples = self
            .bytes
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<i16>>();

        /*
        let samples = self
            .bytes
            .iter()
            .map(|val| i8::from_le_bytes([*val]))
            .map(|int| match int < 0 {
                true => int as f32 / (i8::MIN as f32),
                false => int as f32 / (i8::MAX as f32),
            })
            .collect::<Vec<f32>>();
        */

        wavers::write(
            out_path,
            &samples,
            (self.format.samples_per_sec) as i32,
            self.format.num_channels.into(),
        )
        .map_err(|_| io::Error::other("Failed to write wav file."))
    }

    pub fn dump_raw<P: AsRef<Path>>(&self, out_path: P) -> Result<(), io::Error> {
        fs::create_dir_all(out_path.as_ref().parent().unwrap())?;

        fs::write(out_path, &self.bytes)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use binrw::BinWrite;

    use super::*;

    #[test]
    fn wavebank_mini_format_de_mono() {
        let dword = u32::from_le_bytes([0x44, 0xc4, 0x0a, 0x80]);

        let format = WaveBankMiniWaveFormat3::from(dword);
        dbg!(&format);

        assert_eq!(format.num_channels, 1, "Should be mono.");
        assert_eq!(format.format_tag, 0, "Format should be PCM.");
        assert_eq!(
            format.samples_per_sec, 22050,
            "Sample rate should be 22050."
        );

        /*
        assert_eq!(
            format.samples_per_sec, 44100,
            "Sample rate should be 44100."
        );

        */
        assert!(format.uses_wide_format, "Wide format should be true.")
    }

    #[test]
    fn wavebank_mini_format_de_stereo() {
        let dword = u32::from_le_bytes([0x88, 0x88, 0x15, 0x80]);

        let format = WaveBankMiniWaveFormat3::from(dword);
        dbg!(&format);

        assert_eq!(format.num_channels, 2, "Should be stereo.");
        assert_eq!(format.format_tag, 0, "Shouldn't be compressed.");
        assert_eq!(
            format.samples_per_sec, 44100,
            "Sample rate should be 44100."
        );

        assert!(format.uses_wide_format, "Wide format should be true.")
    }

    #[test]
    fn xwb_se_dese() -> Result<(), crate::Error> {
        let xwb = include_bytes!("xsb/test.xwb");

        let xwavebank = std::io::Cursor::new(xwb).read_le::<XWavebank>()?;

        let mut serialised = vec![];

        xwavebank.write_le(&mut std::io::Cursor::new(&mut serialised))?;

        crate::utils::compare_streams(xwb, &serialised);

        Ok(())
    }
}
