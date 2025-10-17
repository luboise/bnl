use std::{
    error::Error,
    fs,
    io::{self, Cursor, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use byteorder::{LittleEndian, ReadBytesExt};
use serde::Deserialize;

pub fn dump_wav_files(wav_files: &[WavFile], dump_dir: PathBuf) -> Result<(), Box<dyn Error>> {
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

pub fn wav_files_from_path(path: PathBuf) -> Result<Vec<WavFile>, Box<dyn Error>> {
    let bytes = fs::read(path)?;

    let mut cur = Cursor::new(&bytes);

    let mut wbnd_string = [0u8; 4];
    cur.read_exact(&mut wbnd_string)?;

    println!("Reading XWavebank header.");

    let header = XWavebankHeader {
        wbnd_string,
        unknown_count_1: cur.read_u32::<LittleEndian>()?,
        header_size: cur.read_u32::<LittleEndian>()?,
        wavebanks_ptr: cur.read_u32::<LittleEndian>()?,
        wav_entries_ptr: cur.read_u32::<LittleEndian>()?,
        wav_entries_size: cur.read_u32::<LittleEndian>()?,
        unknown_count_2: cur.read_u32::<LittleEndian>()?,
        unknown_1: cur.read_u32::<LittleEndian>()?,
        wave_data_ptr: cur.read_u32::<LittleEndian>()?,
        wave_data_length: cur.read_u32::<LittleEndian>()?,
    };

    let num_wav_entries = header.wav_entries_size / (6 * 4);
    println!("Found {} entries.", num_wav_entries);

    let mut wav_files: Vec<WavFile> = vec![];

    let mut raw_wav_entries = vec![RawWavEntry::default(); num_wav_entries as usize];

    // Read wav entries
    if num_wav_entries != 0 {
        cur.seek(SeekFrom::Start(header.wav_entries_ptr as u64))?;

        for i in 0..num_wav_entries as usize {
            let raw_entry = RawWavEntry {
                unknown_1: cur.read_u32::<LittleEndian>()?,

                raw_format: cur.read_u32::<LittleEndian>()?,

                bytes_ptr: cur.read_u32::<LittleEndian>()?,
                num_bytes: cur.read_u32::<LittleEndian>()?,
                unknown_2: cur.read_u32::<LittleEndian>()?,
                unknown_3: cur.read_u32::<LittleEndian>()?,
            };

            raw_wav_entries[i] = raw_entry;
        }
    }

    wav_files.resize(raw_wav_entries.len(), Default::default());

    // Read wav data
    let mut res_cursor = cur.clone();

    println!("Reading wav files.");
    for (i, raw_entry) in raw_wav_entries.into_iter().enumerate() {
        let mut audio_bytes = vec![0u8; raw_entry.num_bytes as usize];

        res_cursor.seek(SeekFrom::Start(
            (raw_entry.bytes_ptr + header.wave_data_ptr) as u64,
        ))?;

        res_cursor.read_exact(&mut audio_bytes)?;

        wav_files[i] = WavFile::from_raw(raw_entry, audio_bytes);
    }

    Ok(wav_files)
}

const XWAVEBANK_HEADER_SIZE: usize = 40;

#[derive(Debug, Deserialize)]
#[repr(C, packed)]
pub(crate) struct XWavebankHeader {
    /// String which just says "WBND" in ASCII
    wbnd_string: [u8; 4],

    unknown_count_1: u32,

    header_size: u32, // Size of a WavebankHeader
    wavebanks_ptr: u32,

    wav_entries_ptr: u32,
    wav_entries_size: u32, // Total size of all the wav entries in bytes

    unknown_count_2: u32,

    unknown_1: u32,

    wave_data_ptr: u32,
    wave_data_length: u32,
}

pub(crate) struct Wavebank {
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
pub(crate) struct RawWavEntry {
    unknown_1: u32,

    raw_format: u32,

    bytes_ptr: u32,
    num_bytes: u32,
    unknown_2: u32,
    unknown_3: u32,
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
    pub(crate) fn from_raw(raw: RawWavEntry, bytes: Vec<u8>) -> Self {
        Self {
            unknown_1: raw.unknown_1,

            format: WaveBankMiniWaveFormat3::new(raw.raw_format),
            bytes,
            unknown_2: raw.unknown_2,
            unknown_3: raw.unknown_3,
        }
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
