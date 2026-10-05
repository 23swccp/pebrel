//! Real bounded GIF cursor. Decoder composition survives revoked presentation tickets.
use gif::{ColorOutput, DecodeOptions, Decoder, DisposalMethod, MemoryLimit, Repeat};
use image::RgbaImage;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_FRAME_BYTES: u64 = 4 * 1024 * 1024;
const MAX_HEADER_METADATA_BYTES: usize = 64 * 1024;

/// Decoder metadata has separate buffers; reject a large header before it retains them.
fn preflight_header(reader: &mut (impl Read + Seek)) -> Result<(), String> {
    let mut header = [0u8; 13];
    reader.read_exact(&mut header).map_err(|e| e.to_string())?;
    if &header[..6] != b"GIF87a" && &header[..6] != b"GIF89a" {
        return Err("invalid GIF signature".into());
    }
    let mut scratch = [0u8; 768];
    if header[10] & 128 != 0 {
        let bytes = 3 * (1usize << ((header[10] & 7) + 1));
        reader.read_exact(&mut scratch[..bytes]).map_err(|e| e.to_string())?;
    }
    let mut metadata = 0usize;
    loop {
        let mut tag = [0u8; 1];
        reader.read_exact(&mut tag).map_err(|e| e.to_string())?;
        match tag[0] {
            0x2c => return Ok(()),
            0x21 => {
                reader.read_exact(&mut tag).map_err(|e| e.to_string())?;
                loop {
                    reader.read_exact(&mut tag).map_err(|e| e.to_string())?;
                    let length = tag[0] as usize;
                    metadata += 1 + length;
                    if metadata > MAX_HEADER_METADATA_BYTES {
                        return Err(
                            "GIF header metadata exceeds admission before decoder allocation"
                                .into(),
                        );
                    }
                    if length == 0 {
                        break;
                    }
                    reader.read_exact(&mut scratch[..length]).map_err(|e| e.to_string())?;
                }
            },
            _ => return Err("GIF header has no admitted image block".into()),
        }
    }
}

/// Prevent an appended file from making a running cursor consume unlimited input.
struct BoundedFile {
    file: File,
    end: u64,
    position: u64,
}

impl Read for BoundedFile {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let allowed = bytes.len().min(self.end.saturating_sub(self.position) as usize);
        let count = self.file.read(&mut bytes[..allowed])?;
        self.position += count as u64;
        Ok(count)
    }
}

impl Seek for BoundedFile {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let position = match from {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
            SeekFrom::End(value) => i128::from(self.end) + i128::from(value),
        };
        if position < 0 || position > i128::from(self.end) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek past admitted GIF input",
            ));
        }
        self.position = self.file.seek(SeekFrom::Start(position as u64))?;
        Ok(self.position)
    }
}

pub struct Decoded {
    pub pixels: RgbaImage,
    pub delay: Duration,
    pub sequence: u64,
}

#[derive(Clone, Copy)]
struct Rect {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

pub struct Cursor {
    reader: Decoder<BufReader<BoundedFile>>,
    path: PathBuf,
    fingerprint: (u64, Option<SystemTime>),
    width: u32,
    height: u32,
    canvas: Vec<u8>,
    patch: Vec<u8>,
    saved: Vec<u8>,
    previous: Option<(Rect, DisposalMethod)>,
    sequence: u64,
    loops_done: u64,
}

fn zeroed(bytes: usize) -> Result<Vec<u8>, String> {
    let mut buffer = Vec::new();
    buffer.try_reserve_exact(bytes).map_err(|error| error.to_string())?;
    buffer.resize(bytes, 0);
    Ok(buffer)
}

impl Cursor {
    pub fn open(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|error| error.to_string())?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        let bytes = metadata.len();
        if bytes == 0 || bytes > MAX_INPUT_BYTES {
            return Err("GIF input exceeds prototype admission".into());
        }
        let mut options = DecodeOptions::new();
        options.set_color_output(ColorOutput::RGBA);
        options.set_memory_limit(MemoryLimit::Bytes(NonZeroU64::new(MAX_FRAME_BYTES).unwrap()));
        options.check_frame_consistency(true);
        options.check_lzw_end_code(true);
        let mut admitted = BufReader::new(BoundedFile { file, end: bytes, position: 0 });
        preflight_header(&mut admitted)?;
        admitted.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let reader = options.read_info(admitted).map_err(|error| error.to_string())?;
        let (width, height) = (u32::from(reader.width()), u32::from(reader.height()));
        let canvas_bytes = u64::from(width) * u64::from(height) * 4;
        if width == 0
            || height == 0
            || width > 1280
            || height > 720
            || canvas_bytes > MAX_FRAME_BYTES
        {
            return Err("GIF canvas exceeds prototype admission before pixel allocation".into());
        }
        Ok(Self {
            reader,
            path: path.to_owned(),
            fingerprint: (bytes, metadata.modified().ok()),
            width,
            height,
            canvas: zeroed(canvas_bytes as usize)?,
            patch: Vec::new(),
            saved: Vec::new(),
            previous: None,
            sequence: 0,
            loops_done: 0,
        })
    }

    /// Explicitly charged canvas/patch/restore capacities; excludes decoder internals.
    pub fn owned_capacity(&self) -> usize {
        self.canvas.capacity() + self.patch.capacity() + self.saved.capacity()
    }
    pub fn loops_done(&self) -> u64 {
        self.loops_done
    }

    pub fn next(&mut self) -> Result<Option<Decoded>, String> {
        let Some(info) = self.reader.next_frame_info().map_err(|error| error.to_string())? else {
            return Ok(None);
        };
        let rect = Rect {
            x: info.left as usize,
            y: info.top as usize,
            width: info.width as usize,
            height: info.height as usize,
        };
        let dispose = info.dispose;
        let delay = if info.delay == 0 {
            Duration::from_millis(100)
        } else {
            Duration::from_millis((u64::from(info.delay) * 10).max(20))
        };
        // Metadata and rectangle are checked before reserving patch storage.
        if rect.width == 0
            || rect.height == 0
            || rect.x + rect.width > self.width as usize
            || rect.y + rect.height > self.height as usize
        {
            return Err("GIF patch exceeds its admitted canvas before pixel allocation".into());
        }
        let patch_bytes = rect.width * rect.height * 4;
        if self.patch.capacity() < patch_bytes {
            self.patch
                .try_reserve_exact(patch_bytes - self.patch.len())
                .map_err(|error| error.to_string())?;
        }
        self.patch.resize(patch_bytes, 0);
        self.reader.read_into_buffer(&mut self.patch).map_err(|error| error.to_string())?;
        // The exact-sized output can fill before the decoder consumes the LZW end code.
        // Drain through a one-pixel sentinel, rejecting pixels beyond the declared rectangle.
        if self.reader.fill_buffer(&mut [0u8; 4]).map_err(|error| error.to_string())? {
            return Err("GIF patch produces more pixels than its admitted rectangle".into());
        }
        let stride = self.width as usize * 4;
        if let Some((old, disposal)) = self.previous {
            for row in 0..old.height {
                let start = (old.y + row) * stride + old.x * 4;
                let target = &mut self.canvas[start..start + old.width * 4];
                match disposal {
                    DisposalMethod::Background => target.fill(0),
                    DisposalMethod::Previous => target.copy_from_slice(
                        &self.saved[row * old.width * 4..(row + 1) * old.width * 4],
                    ),
                    DisposalMethod::Keep | DisposalMethod::Any => {},
                }
            }
        }
        if dispose == DisposalMethod::Previous {
            if self.saved.capacity() < patch_bytes {
                self.saved
                    .try_reserve_exact(patch_bytes - self.saved.len())
                    .map_err(|error| error.to_string())?;
            }
            self.saved.resize(patch_bytes, 0);
            for row in 0..rect.height {
                let start = (rect.y + row) * stride + rect.x * 4;
                self.saved[row * rect.width * 4..(row + 1) * rect.width * 4]
                    .copy_from_slice(&self.canvas[start..start + rect.width * 4]);
            }
        }
        for row in 0..rect.height {
            let start = (rect.y + row) * stride + rect.x * 4;
            for (target, source) in self.canvas[start..start + rect.width * 4]
                .chunks_exact_mut(4)
                .zip(self.patch[row * rect.width * 4..(row + 1) * rect.width * 4].chunks_exact(4))
            {
                if source[3] != 0 {
                    target.copy_from_slice(source);
                }
            }
        }
        self.previous = Some((rect, dispose));
        let mut bgra = zeroed(self.canvas.len())?;
        for (target, rgba) in bgra.chunks_exact_mut(4).zip(self.canvas.chunks_exact(4)) {
            target.copy_from_slice(&[rgba[2], rgba[1], rgba[0], rgba[3]]);
        }
        self.sequence += 1;
        Ok(Some(Decoded {
            pixels: RgbaImage::from_raw(self.width, self.height, bgra).unwrap(),
            delay,
            sequence: self.sequence,
        }))
    }

    pub fn next_looping(&mut self) -> Result<Option<Decoded>, String> {
        if let Some(frame) = self.next()? {
            return Ok(Some(frame));
        }
        let repeat = self.reader.repeat();
        if self.sequence == 0
            || matches!(repeat, Repeat::Finite(n) if self.loops_done >= u64::from(n))
        {
            return Ok(None);
        }
        // Loop restart is a worker operation. Metadata is a change detector, not a content hash.
        let mut restarted = Self::open(&self.path)?;
        if restarted.fingerprint != self.fingerprint {
            return Err("GIF source changed between loops".into());
        }
        restarted.sequence = self.sequence;
        restarted.loops_done = self.loops_done + 1;
        *self = restarted;
        self.next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new(bytes: &[u8]) -> Self {
            let path = fixtures().join(format!(
                "owned-gif-test-{}-{}.gif",
                std::process::id(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
            ));
            let mut file =
                std::fs::OpenOptions::new().create_new(true).write(true).open(&path).unwrap();
            std::io::Write::write_all(&mut file, bytes).unwrap();
            Self(path)
        }
        fn looping(repeat: Repeat) -> Self {
            let mut bytes = Vec::new();
            {
                let mut encoder =
                    gif::Encoder::new(&mut bytes, 4, 4, &[255, 0, 0, 0, 255, 0]).unwrap();
                encoder.set_repeat(repeat).unwrap();
                for color in [0, 1] {
                    encoder
                        .write_frame(&gif::Frame {
                            width: 4,
                            height: 4,
                            delay: 4,
                            dispose: DisposalMethod::Keep,
                            buffer: std::borrow::Cow::Owned(vec![color; 16]),
                            ..Default::default()
                        })
                        .unwrap();
                }
            }
            Self::new(&bytes)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_file(&self.0).unwrap();
        }
    }
    fn fixtures() -> std::path::PathBuf {
        std::env::var_os("PEBREL_MEDIA_FIXTURE_DIR")
            .expect("set generated fixture directory")
            .into()
    }
    #[test]
    fn actual_disposal_and_transparency_match_the_independent_oracle() {
        let root = fixtures();
        let oracle = std::fs::read(root.join("disposal-oracle.rgba")).unwrap();
        let mut cursor = Cursor::open(&root.join("disposal.gif")).unwrap();
        for index in 0..5 {
            let frame = cursor.next().unwrap().unwrap();
            assert_eq!(frame.sequence, index + 1);
            assert_eq!(frame.delay, Duration::from_millis(40));
            for (bgra, rgba) in frame
                .pixels
                .as_raw()
                .chunks_exact(4)
                .zip(oracle[index as usize * 64..(index as usize + 1) * 64].chunks_exact(4))
            {
                assert_eq!(bgra[3], rgba[3]);
                if rgba[3] != 0 {
                    assert_eq!(bgra, &[rgba[2], rgba[1], rgba[0], rgba[3]]);
                }
            }
        }
        assert!(cursor.next().unwrap().is_none());
    }
    #[test]
    fn decoder_progress_survives_discarding_an_unpresentable_output() {
        let mut cursor = Cursor::open(&fixtures().join("disposal.gif")).unwrap();
        drop(cursor.next().unwrap().unwrap());
        drop(cursor.next().unwrap().unwrap());
        let after_previous = cursor.next().unwrap().unwrap();
        assert_eq!(after_previous.sequence, 3);
        assert_eq!(after_previous.pixels.get_pixel(1, 1).0, [0, 0, 255, 255]);
        assert_eq!(after_previous.pixels.get_pixel(2, 1).0, [255, 0, 0, 255]);
    }
    #[test]
    fn a_long_zero_delay_stream_is_consumed_incrementally() {
        let mut cursor = Cursor::open(&fixtures().join("zero-delay-10000.gif")).unwrap();
        for sequence in 1..=10_000 {
            let frame = cursor.next().unwrap().unwrap();
            assert_eq!(frame.sequence, sequence);
            assert_eq!(frame.delay, Duration::from_millis(100));
            assert_eq!(frame.pixels.as_raw().capacity(), 64);
            assert!(cursor.owned_capacity() <= 192, "long streams do not retain every frame");
            drop(frame);
        }
        assert!(cursor.next().unwrap().is_none());
    }
    #[test]
    fn oversized_canvas_is_rejected_at_header_admission() {
        let result = Cursor::open(&fixtures().join("large-canvas-header.gif"));
        assert!(matches!(result, Err(ref error) if error.contains("before pixel allocation")));
    }
    #[test]
    fn corrupt_input_returns_a_decoder_error() {
        assert!(Cursor::open(&fixtures().join("invalid.gif")).is_err());
    }
    #[test]
    fn cursor_can_be_transferred_without_collecting_animation_frames() {
        fn assert_send<T: Send>() {}
        assert_send::<Cursor>();
    }
    #[test]
    fn full_size_stream_consumes_end_codes_and_matches_independent_pixel_samples() {
        let oracle: serde_json::Value = serde_json::from_slice(
            &std::fs::read(fixtures().join("gif-720p-oracle-samples.json")).unwrap(),
        )
        .unwrap();
        let mut cursor = Cursor::open(&fixtures().join("animated-720p12.gif")).unwrap();
        for samples in oracle["frames"].as_array().unwrap() {
            let frame = cursor.next().unwrap().unwrap();
            for (index, expected) in samples.as_array().unwrap().iter().enumerate() {
                let pixel = frame
                    .pixels
                    .get_pixel((index as u32 * 158 + 37) % 1280, (index as u32 * 89 + 23) % 720);
                assert_eq!(
                    pixel.0,
                    [
                        expected[0].as_u64().unwrap() as u8,
                        expected[1].as_u64().unwrap() as u8,
                        expected[2].as_u64().unwrap() as u8,
                        expected[3].as_u64().unwrap() as u8
                    ]
                );
            }
        }
        assert!(cursor.next().unwrap().is_none());
    }
    #[test]
    fn patch_outside_logical_canvas_is_rejected_before_patch_storage() {
        let original = std::fs::read(fixtures().join("disposal.gif")).unwrap();
        let mut bytes = original;
        let descriptor =
            bytes.windows(10).position(|b| b == [0x2c, 0, 0, 0, 0, 4, 0, 4, 0, 0]).unwrap();
        bytes[descriptor + 5..descriptor + 7].copy_from_slice(&65535u16.to_le_bytes());
        let file = Fixture::new(&bytes);
        let mut cursor = Cursor::open(&file.0).unwrap();
        let initial = cursor.owned_capacity();
        assert!(cursor.next().is_err());
        assert_eq!(cursor.owned_capacity(), initial);
    }
    #[test]
    fn finite_loop_restarts_composition_and_preserves_monotonic_sequence() {
        let file = Fixture::looping(Repeat::Finite(1));
        let mut cursor = Cursor::open(&file.0).unwrap();
        for sequence in 1..=4 {
            let frame = cursor.next_looping().unwrap().unwrap();
            assert_eq!(frame.sequence, sequence);
            assert_eq!(
                frame.pixels.get_pixel(0, 0).0,
                if sequence % 2 == 1 { [0, 0, 255, 255] } else { [0, 255, 0, 255] }
            );
        }
        assert!(cursor.next_looping().unwrap().is_none());
        assert_eq!(cursor.loops_done(), 1);
    }
    #[test]
    fn infinite_loops_stay_bounded() {
        let file = Fixture::looping(Repeat::Infinite);
        let mut cursor = Cursor::open(&file.0).unwrap();
        for sequence in 1..=200 {
            assert_eq!(cursor.next_looping().unwrap().unwrap().sequence, sequence);
            assert!(cursor.owned_capacity() <= 192);
        }
        assert_eq!(cursor.loops_done(), 99);
    }
    #[test]
    fn source_change_on_loop_is_reported_instead_of_silently_mixing_versions() {
        let file = Fixture::looping(Repeat::Infinite);
        let mut cursor = Cursor::open(&file.0).unwrap();
        cursor.next_looping().unwrap().unwrap();
        cursor.next_looping().unwrap().unwrap();
        let mut changed = std::fs::read(&file.0).unwrap();
        changed.push(0);
        std::fs::write(&file.0, changed).unwrap();
        assert!(
            matches!(cursor.next_looping(), Err(ref error) if error.contains("source changed"))
        );
    }
    #[test]
    fn bounded_reader_ignores_appended_bytes_and_rejects_outside_seeks() {
        let file = Fixture::new(&[1, 2, 3]);
        let mut reader = BoundedFile { file: File::open(&file.0).unwrap(), end: 2, position: 0 };
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, [1, 2]);
        assert!(reader.seek(SeekFrom::Start(3)).is_err());
        assert!(reader.seek(SeekFrom::End(1)).is_err());
        assert!(reader.seek(SeekFrom::Current(-3)).is_err());
    }
    #[test]
    fn large_application_metadata_is_rejected_before_decoder_retention() {
        let file = Fixture::looping(Repeat::Finite(1));
        let original = std::fs::read(&file.0).unwrap();
        // Inject an ICC application extension immediately after the global palette.
        let palette = 3 * (1usize << ((original[10] & 7) + 1));
        let start = 13 + palette;
        let mut metadata = vec![0x21, 0xff, 11];
        metadata.extend_from_slice(b"ICCRGBG1012");
        for _ in 0..260 {
            metadata.push(255);
            metadata.extend_from_slice(&[0u8; 255]);
        }
        metadata.push(0);
        let mut injected = original[..start].to_vec();
        injected.extend_from_slice(&metadata);
        injected.extend_from_slice(&original[start..]);
        let oversized = Fixture::new(&injected);
        assert!(
            matches!(Cursor::open(&oversized.0),Err(ref error) if error.contains("metadata exceeds"))
        );
    }
}
