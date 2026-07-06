use std::{
    fs,
    io::{self, Cursor, Read, SeekFrom},
    path::{Path, PathBuf},
};

use binrw::BinReaderExt;
use serde::Deserialize;

pub fn dump_wav_files(wav_files: &[WavFile], dump_dir: PathBuf) -> Result<(), crate::Error> {
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

pub fn wav_files_from_path(path: PathBuf) -> Result<Vec<WavFile>, crate::Error> {
    let bytes = fs::read(path)?;

    let mut cur = Cursor::new(&bytes);

    let wavebank: XWavebank = cur.read_le()?;
    println!("Found {} entries.", wavebank.wav_entries.len());

    let wav_files = wavebank
        .wav_entries
        .iter()
        .map(|raw| WavFile::from_raw(raw.clone(), &wavebank.wave_data))
        .collect::<Result<Vec<_>, crate::Error>>()?;

    Ok(wav_files)
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

    #[br(restore_position,
        seek_before = SeekFrom::Start(wav_entries_ptr.into()),
        count = wav_entries_size / 24)]
    pub wav_entries: Vec<RawWavEntry>,

    unknown_count_2: u32,

    unknown_1: u32,

    #[br(temp)]
    wave_data_ptr: u32,
    #[br(temp)]
    wave_data_length: u32,

    #[br(restore_position, count = wave_data_length, seek_before = SeekFrom::Start(wave_data_ptr.into()))]
    pub wave_data: Vec<u8>,
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

impl WaveBankMiniWaveFormat1 {
    fn new(dword: u32) -> Self {
        // DWORD wFormatTag : 1;
        let is_compressed: bool = dword & 0x1 == 1;

        // DWORD nChannels : 3;
        let num_channels: u8 = (dword >> 1) as u8 & 0b111;

        // DWORD nSamplesPerSec : 27;
        let samples_per_sec: u32 = (dword >> (1 + 3)) & 0x1FFFFFF; // 27 bits

        // DWORD wBitsPerSample : 1;
        let uses_wide_format: bool = ((dword >> (1 + 3 + 27)) & 1u32) == 1;

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
#[derive(Debug, Clone)]
pub struct WaveBankMiniWaveFormat3 {
    /// DWORD wFormatTag : 2
    format_tag: u8,
    /// DWORD nChannels : 3;
    num_channels: u8,
    /// DWORD nSamplesPerSec : 18;
    samples_per_sec: u32,
    /// DWORD wBlockAlign    :  8;
    block_align: u8,
    /// DWORD wBitsPerSample :  1;
    uses_wide_format: bool,
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

impl WaveBankMiniWaveFormat3 {
    fn new(dword: u32) -> Self {
        let format_tag: u8 = (dword & 0b11) as u8;

        // DWORD nChannels : 3
        let num_channels: u8 = (dword >> 2) as u8 & 0b111;

        // DWORD nSamplesPerSec : 18;
        let samples_per_sec: u32 = (dword >> (2 + 3)) & 0x3FFFF; // 18 bits

        let block_align: u8 = (dword >> (2 + 3 + 18)) as u8; // 8 bits

        // DWORD wBitsPerSample : 1;
        let uses_wide_format: bool = ((dword >> (2 + 3 + 18 + 8)) & 1u32) == 1;

        Self {
            format_tag,
            num_channels,
            samples_per_sec,
            block_align,
            uses_wide_format,
        }
    }
}

#[derive(Debug, Deserialize, Default, Clone)]
#[binrw::binrw]
#[br(little)]
#[bw(little)]
pub struct RawWavEntry {
    pub unknown_1: u32,

    pub raw_format: u32,

    pub bytes_ptr: u32,
    pub num_bytes: u32,
    pub unknown_2: u32,
    pub unknown_3: u32,
}

#[derive(Default, Clone)]
pub struct WavFile {
    unknown_1: u32,

    format: WaveBankMiniWaveFormat3,

    bytes: Vec<u8>,

    unknown_2: u32,
    unknown_3: u32,
}

impl WavFile {
    pub fn from_raw(raw: RawWavEntry, bytes: &[u8]) -> Result<Self, crate::Error> {
        let bytes = bytes
            .get(raw.bytes_ptr as usize..(raw.bytes_ptr + raw.num_bytes) as usize)
            .ok_or("bad slice")?
            .to_vec();

        Ok(Self {
            unknown_1: raw.unknown_1,

            format: WaveBankMiniWaveFormat3::new(raw.raw_format),
            bytes,
            unknown_2: raw.unknown_2,
            unknown_3: raw.unknown_3,
        })
    }

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
            (self.format.samples_per_sec / self.format.num_channels as u32) as i32,
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
    use super::*;

    #[test]
    fn wavebank_mini_format_de_mono() {
        let dword = u32::from_le_bytes([0x44, 0xc4, 0x0a, 0x80]);

        let format = WaveBankMiniWaveFormat3::new(dword);
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

        let format = WaveBankMiniWaveFormat3::new(dword);
        dbg!(&format);

        assert_eq!(format.num_channels, 2, "Should be stereo.");
        assert_eq!(format.format_tag, 0, "Shouldn't be compressed.");
        assert_eq!(
            format.samples_per_sec, 44100,
            "Sample rate should be 44100."
        );

        assert!(format.uses_wide_format, "Wide format should be true.")
    }
}
