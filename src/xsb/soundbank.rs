use std::io::SeekFrom;

use binrw::BinReaderExt;

#[binrw::binrw]
#[derive(Debug, Clone)]
pub struct CueEntry {
    // TODO: implement later
    pub flags: u16,
    pub sound_index: u16,
    pub name_ptr: u32,
    pub a: u32,
    pub b: u32,
    pub c: u32,
}

#[repr(u8)]
pub enum XsbSoundFlag {
    /// XSB_SOUND_FLAG_TRIVIAL
    Trivial = 1 << 3,
    /// XSB_SOUND_FLAG_SIMPLE
    Simple = 1 << 4,
}

#[derive(Debug, Clone)]
#[binrw::binrw]
pub struct SoundEntryData {}

#[binrw::binread]
#[derive(Debug, Clone)]
pub struct TrivialSound {
    pub bank_index: BankIndex,
}

#[binrw::binread]
#[derive(Debug, Clone)]
pub struct SimpleSound {
    #[br(temp)]
    variations_ptr: u32,
    #[br(restore_position, seek_before = SeekFrom::Start(variations_ptr.into()))]
    pub variations: SimpleWaveVariations,
}

#[repr(u8)]
pub enum PlayEventBits {
    Complex = 0x04,
    LoopVariation = 0x40,
}

#[binrw::binrw]
#[brw(repr = u8)]
#[repr(u8)]
#[derive(num_enum::TryFromPrimitive, Debug, Clone, PartialEq, Eq)]
pub enum ComplexEventType {
    Play = 0x00,
    PlayComplex = 0x01,
    EnvelopeAmplitude = 0x0a,
    Disabled = 0x0f,
    MixBinSpan = 0x10,
}

#[derive(Debug, Clone)]
pub struct VariationParams {
    pub flag1: bool,
    pub flag2: bool,
    pub current_variation: u16,         // u13,
    pub variation_selection_method: u8, // u4
    pub num_variations: u16,            // u13
}

impl From<u32> for VariationParams {
    fn from(value: u32) -> Self {
        let flag1 = value & (1 << 31) > 0;
        let flag2 = value & (1 << 30) > 0;

        let current_variation = (value.unbounded_shr(17) as u16) & 0b1111111111111;
        let variation_selection_method = (value.unbounded_shr(13) as u8) & 0b1111;
        let num_variations = (value as u16) & 0b1111111111111;

        Self {
            flag1,
            flag2,
            current_variation,
            variation_selection_method,
            num_variations,
        }
    }
}

#[binrw::binread]
#[derive(Debug, Clone)]
pub struct SimpleWaveVariations {
    #[br(map = |x: [u8; 4]| u32::from_le_bytes(x).into())]
    pub variation_params: VariationParams,
    #[br(count = variation_params.num_variations)]
    pub variations: Vec<BankIndex>,
}

#[binrw::binread]
#[derive(Debug, Clone)]
pub struct ComplexWaveVariations {
    #[br(map = |x: [u8; 4]| u32::from_le_bytes(x).into())]
    pub variation_params: VariationParams,
    #[br(count = variation_params.num_variations)]
    pub variations: Vec<ComplexVariation>,
}

#[binrw::binread]
#[derive(Debug, Clone)]
pub struct BankIndex {
    /// Index of sound in referenced wavebank
    pub wave_index: u16,
    /// Index of referenced wavebank in XSoundbank::wavebank_entries
    #[br(assert(wavebank_index < 10))]
    pub wavebank_index: u16,
}

#[binrw::binread]
#[derive(Debug, Clone)]
pub struct ComplexVariation {
    pub bank_index: BankIndex,
    pub weight_min: u16,
    pub weight_max: u16,
}

#[binrw::binread]
#[derive(Debug, Clone)]
#[br(import(event_type: ComplexEventType, event_flags: u8, params_size: u8))]
pub enum ComplexEventParams {
    #[br(assert(event_type == ComplexEventType::Play
            && ((event_flags & PlayEventBits::Complex as u8) == 0)))]
    Play(BankIndex),
    #[br(assert(event_type == ComplexEventType::Play
            && ((event_flags & PlayEventBits::Complex as u8) != 0)))]
    PlayVaried {
        #[br(temp)]
        wave_variations_ptr: u32,
        #[br(restore_position, seek_before = SeekFrom::Start(wave_variations_ptr.into()))]
        wave_variations: ComplexWaveVariations,
    },
    #[br(assert(event_type == ComplexEventType::PlayComplex
            && ((event_flags & PlayEventBits::Complex as u8) == 0)))]
    PlayComplex {
        bank_index: BankIndex,
        other_stuff: [u8; 12],
    },
    #[br(assert(event_type == ComplexEventType::PlayComplex
            && ((event_flags & PlayEventBits::Complex as u8) != 0)))]
    PlayComplexVaried {
        #[br(temp)]
        wave_variations_ptr: u32,
        #[br(restore_position, seek_before = SeekFrom::Start(wave_variations_ptr.into()))]
        wave_variations: ComplexWaveVariations,
        other_stuff: [u8; 12],
    },
    #[br(assert(event_type == ComplexEventType::EnvelopeAmplitude))]
    EnvelopeAmplitude {
        delay_seconds: u16,
        attack_seconds: u16,
        hold_seconds: u16,
        decay_seconds: u16,
        release_seconds: u16,
        sustain_power: u8,
        unknown: u8,
    },
    #[br(assert(event_type == ComplexEventType::Disabled))]
    Disabled(),
    #[br(assert(event_type == ComplexEventType::MixBinSpan))]
    MixBinSpan {
        speaker_configuration: u8,
        #[br(map = from_u24_map)]
        angle_and_flag: u32,

        channel_index_0: u8,
        channel_unknown_0: u8,
        channel_volume_0: u16,

        channel_index_1: u8,
        channel_unknown_1: u8,
        channel_volume_1: u16,

        channel_index_2: u8,
        channel_unknown_2: u8,
        channel_volume_2: u16,

        channel_index_3: u8,
        channel_unknown_3: u8,
        channel_volume_3: u16,

        #[br(count = params_size - 20)]
        leftover_params: Vec<u8>,
    },
}

pub fn from_u24_map(bytes: [u8; 3]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0])
}

#[binrw::binread]
#[derive(Debug, Clone)]
pub struct TrackEvent {
    #[br(temp)]
    event_type: ComplexEventType,
    // event type contained by ComplexEventParams
    #[br(map = from_u24_map)]
    pub timestamp_ms: u32,
    #[br(temp)]
    parameter_size: u8,
    pub flags: u8,
    pub unknown_u16: u16,
    #[br(args(event_type, flags, parameter_size))]
    pub params: ComplexEventParams,
}

#[binrw::binread]
#[derive(Debug, Clone)]
pub struct ComplexTrack {
    #[br(temp)]
    num_events: u8,

    #[br(temp, map = from_u24_map)]
    events_ptr: u32,

    #[br(count = num_events, restore_position, seek_before = SeekFrom::Start(events_ptr.into()))]
    pub events: Vec<TrackEvent>,
}

#[binrw::binread]
#[derive(Debug, Clone)]
pub struct ComplexSound {
    #[br(temp)]
    pub complex_ptr: u32,
    #[br(restore_position, seek_before = SeekFrom::Start(complex_ptr.into()))]
    pub sound: ComplexTrack,
}

#[derive(Debug, Clone)]
#[binrw::binread]
#[expect(nonstandard_style)]
pub struct SoundEntry {
    sound_u32: [u8; 4],
    pub volume: u16,
    pub pitch: u16,
    pub trackCount: u8,
    pub layer: u8,
    pub category: u8,
    pub flags: u8,
    pub parameters3DIndex: u16,
    pub priority: u8,
    pub i3dl2Volume: u8,
    pub eqGain: u16,
    pub eqQ: u16,

    #[br(restore_position, args(sound_u32, flags, trackCount))]
    pub sound: Sound,
}

#[derive(Debug, Clone)]
pub enum Sound {
    Trivial(BankIndex),
    // Non-trivial sounds
    // Simple(/* simple variations */ VariationParams, Vec<BankIndex>),
    Simple(ComplexWaveVariations),
    Complex(/* complex variations */ Vec<ComplexTrack>),
}

impl binrw::BinRead for Sound {
    type Args<'a> = ([u8; 4], u8, u8);

    fn read_options<R: std::io::prelude::Read + std::io::prelude::Seek>(
        reader: &mut R,
        _: binrw::Endian,
        args: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<Self> {
        let pos = reader.stream_position()?;

        let (sound_u32, sound_flags, num_tracks) = args;

        if sound_flags & XsbSoundFlag::Trivial as u8 > 0 {
            reader.seek(SeekFrom::Start(pos))?;

            let wave_index = u16::from_le_bytes([sound_u32[0], sound_u32[1]]);
            let wavebank_index = u16::from_le_bytes([sound_u32[2], sound_u32[3]]);

            if wavebank_index > 10 {
                return Err(binrw::Error::AssertFail {
                    pos: reader.stream_position().unwrap_or_default(),
                    message: format!("wavebank index {wavebank_index} is too large"),
                });
            }

            return Ok(Sound::Trivial(BankIndex {
                wave_index,
                wavebank_index,
            }));
        }

        // non-trivial => theres a pointer
        reader.seek(SeekFrom::Start(u32::from_le_bytes(sound_u32).into()))?;

        // simple => no track to read, just get variation table
        if sound_flags & XsbSoundFlag::Simple as u8 > 0 {
            return reader.read_le();
            /*
            let params: VariationParams = reader.read_le::<u32>()?.into();
            let variations = (0..params.num_variations)
                .map(|_| reader.read_le())
                .collect::<Result<_, _>>()?;

            reader.seek(SeekFrom::Start(pos))?;
            return Ok(Sound::Simple(params, variations));
            */
        }

        // complex => get all tracks
        let tracks = (0..num_tracks)
            .map(|_| reader.read_le())
            .collect::<Result<_, _>>()?;

        // reset and return
        reader.seek(SeekFrom::Start(pos))?;
        Ok(Sound::Complex(tracks))
    }
}

/*
struct Sound {
    u32 flags;
    u32 idk1;
    float f1;
    float f2;
    float f3;
    float f4;
    // float f5;
};

struct SoundTable {
    u32 flags;
    u32 idk1;
    float soundFloats[5];

    u32 idk2;
    u32 idk3;
    u32 idk4;

    CueU32 cues[parent.numEntries1];
    Sound sounds[parent.numEntries2];
};
*/

#[binrw::binread]
#[derive(Debug, Clone)]
#[br(import(count: usize))]
#[expect(clippy::manual_non_exhaustive)]
pub struct WavebankArray {
    #[br(count = count)]
    pub names: Vec<super::WavebankName>,

    #[br(magic = b"Null")]
    _null: (),
}

#[binrw::binread]
#[derive(Debug, Clone)]
#[expect(clippy::manual_non_exhaustive)]
pub struct XSoundbank {
    #[brw(magic = b"SDBK")]
    _sdbk: (),

    #[br(assert(version == 11))]
    pub version: u16,

    pub crc: u16,
    #[br(temp)]
    wavebank_array_ptr: u32,

    pub cue_table_ptr: u32,

    // 0x10
    #[br(temp)]
    soundsPtr: u32,
    #[br(temp)]
    fileSize: u32,

    pub xsb_flags: u16,

    pub num_something: u16,

    #[br(temp)]
    num_sounds: u16,
    #[br(temp)]
    num_cues: u16,

    // 0x20
    pub num_entries4: u16,
    #[br(temp)]
    num_wavebanks_used: u16,

    pub short7: u16,
    pub short8: u16,
    pub name: super::SoundbankName,

    #[br(count = num_cues)]
    pub cue_entries: Vec<CueEntry>,
    #[br(count = num_sounds)]
    pub sound_entries: Vec<SoundEntry>,

    // Parsed fields
    #[br(args(num_wavebanks_used.into()), restore_position, seek_before = SeekFrom::Start(wavebank_array_ptr.into()))]
    pub wavebank_array: WavebankArray,
}
