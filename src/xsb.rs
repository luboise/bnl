use std::{
    error::Error,
    fs,
    io::{self, Cursor, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use byteorder::{LittleEndian, ReadBytesExt};
use serde::Deserialize;

pub fn dump_xwavebank_bytes(path: PathBuf, dump_dir: PathBuf) -> Result<(), Box<dyn Error>> {
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
                flag1: cur.read_u8()?,
                flag2: cur.read_u8()?,
                flag3: cur.read_u8()?,
                flag4: cur.read_u8()?,
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

    for (i, wav) in wav_files.iter().enumerate() {
        let out_path = dump_dir.join(format!("wavebank_{}.wav", i));
        println!("Dumping to {}", out_path.display());

        wav.dump(out_path)?;

        let raw_out_path = dump_dir.join(format!("wavebank_raw_{}", i));
        wav.dump_raw(raw_out_path)?;
    }

    Ok(())
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

#[derive(Debug, Deserialize, Default, Clone)]
pub(crate) struct RawWavEntry {
    unknown_1: u32,

    flag1: u8,
    flag2: u8,
    flag3: u8,
    flag4: u8,

    bytes_ptr: u32,
    num_bytes: u32,
    unknown_2: u32,
    unknown_3: u32,
}

#[derive(Default, Clone)]
pub struct WavFile {
    unknown_1: u32,

    flag1: u8,
    flag2: u8,
    flag3: u8,
    flag4: u8,

    bytes: Vec<u8>,

    unknown_2: u32,
    unknown_3: u32,
}

impl WavFile {
    pub(crate) fn from_raw(raw: RawWavEntry, bytes: Vec<u8>) -> Self {
        Self {
            unknown_1: raw.unknown_1,
            flag1: raw.flag1,
            flag2: raw.flag2,
            flag3: raw.flag3,
            flag4: raw.flag4,
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

        wavers::write(out_path, &samples, 44100, 1)
            .map_err(|_| io::Error::other("Failed to write wav file."))
    }

    pub fn dump_raw<P: AsRef<Path>>(&self, out_path: P) -> Result<(), io::Error> {
        fs::create_dir_all(out_path.as_ref().parent().unwrap())?;

        fs::write(out_path, &self.bytes)?;

        Ok(())
    }
}
