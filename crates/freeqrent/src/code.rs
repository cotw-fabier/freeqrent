use alloc::vec::Vec;

use crate::bits::BitBuffer;
use crate::check::RenderError;
use crate::error::EncodeError;
use crate::gf;
#[cfg(feature = "halftone")]
use crate::halftone::{self, Halftone};
use crate::matrix::{Mask, QrMatrix};
#[cfg(feature = "halftone")]
use crate::scene;
#[cfg(feature = "halftone")]
use crate::steer::Want;
#[cfg(not(feature = "halftone"))]
type Want = core::convert::Infallible;
use crate::segment::{self, ECI_UTF8, EciPolicy, Segment};
use crate::style::Style;
use crate::version::{BlockLayout, Ecc, TrackingGrid, Version};

/// Entry point for encoding.
///
/// ```
/// use freeqrent::{Ecc, QrCode};
///
/// let qr = QrCode::encode("https://example.com", Ecc::M).unwrap();
/// assert_eq!(qr.version().number(), 2);
/// ```
#[derive(Clone, Copy, Debug)]
pub struct QrCode;

impl QrCode {
    /// Encodes text with the given minimum ECC level and default options.
    pub fn encode(text: &str, ecc: Ecc) -> Result<QrMatrix, EncodeError> {
        QrCode::builder().ecc(ecc).encode_text(text)
    }

    /// Options for version range, ECC, mask, and ECI.
    pub fn builder() -> QrBuilder {
        QrBuilder::default()
    }
}

/// Encoding options. Plain data; build one with [`QrCode::builder`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QrBuilder {
    ecc: Ecc,
    min_version: Version,
    max_version: Version,
    boost_ecc: bool,
    mask: Option<Mask>,
    eci: EciPolicy,
}

impl Default for QrBuilder {
    fn default() -> QrBuilder {
        QrBuilder {
            ecc: Ecc::M,
            min_version: Version::MIN,
            max_version: Version::MAX,
            boost_ecc: true,
            mask: None,
            eci: EciPolicy::Auto,
        }
    }
}

impl QrBuilder {
    /// Minimum ECC level (default `M`).
    pub fn ecc(mut self, ecc: Ecc) -> QrBuilder {
        self.ecc = ecc;
        self
    }

    /// Smallest version to use. Set it equal to `max_version` to pin the size.
    pub fn min_version(mut self, version: Version) -> QrBuilder {
        self.min_version = version;
        self
    }

    /// Largest version to use (default 40).
    pub fn max_version(mut self, version: Version) -> QrBuilder {
        self.max_version = version;
        self
    }

    /// Use exactly this version.
    pub fn version(self, version: Version) -> QrBuilder {
        self.min_version(version).max_version(version)
    }

    /// Use one of the versions of an art-mode tracking grid (SPEC §7.8);
    /// the smallest that fits is picked.
    pub fn tracking_grid(self, grid: TrackingGrid) -> QrBuilder {
        let (min, max) = grid.versions();
        self.min_version(min).max_version(max)
    }

    /// Raise the ECC level as far as it goes without growing the version
    /// (default `true`).
    pub fn boost_ecc(mut self, boost: bool) -> QrBuilder {
        self.boost_ecc = boost;
        self
    }

    /// Force a mask instead of picking the lowest-penalty one.
    pub fn mask(mut self, mask: Mask) -> QrBuilder {
        self.mask = Some(mask);
        self
    }

    /// ECI header policy for [`encode_text`](Self::encode_text) (default `Auto`).
    pub fn eci(mut self, eci: EciPolicy) -> QrBuilder {
        self.eci = eci;
        self
    }

    /// Encodes UTF-8 text, split into the shortest mix of modes
    /// ([`Segment::optimal`]), with an ECI header if the policy calls for one.
    pub fn encode_text(&self, text: &str) -> Result<QrMatrix, EncodeError> {
        let plans = self.plans(text);
        self.encode_with(|v| &plans[v.count_band()], None)
    }

    /// Count-field widths change at versions 10 and 27, and the best split
    /// can change with them, so text is planned once per band in range.
    fn plans(&self, text: &str) -> [Vec<Segment>; 3] {
        core::array::from_fn(|band| {
            let first = Version::BAND_STARTS[band];
            let in_range = Version::all()
                .filter(|v| v.count_band() == band)
                .any(|v| v >= self.min_version && v <= self.max_version);
            if !in_range {
                return Vec::new();
            }
            let mut segments = Segment::optimal(text, first);
            if self.eci.applies_to(&segments) {
                segments.insert(0, Segment::eci(ECI_UTF8).expect("26 is a valid designator"));
            }
            segments
        })
    }

    /// Encodes text with the free data bits after the payload chosen so the
    /// modules follow `halftone`'s picture (SPEC §7.7 A), and the mask that
    /// matches it best (unless one is forced). The ECC level is used as
    /// given, never boosted: a higher level would trade steerable bits for
    /// error correction the picture doesn't spend. With steering off this
    /// is [`encode_text`](Self::encode_text) without the boost.
    #[cfg(feature = "halftone")]
    pub fn encode_text_steered(
        &self,
        text: &str,
        halftone: &Halftone,
    ) -> Result<QrMatrix, EncodeError> {
        let plans = self.plans(text);
        self.steered(|v| &plans[v.count_band()], halftone, &Style::new())
    }

    /// Steers for `halftone` as drawn by `style`: the picture over its
    /// background, and the modules it clears wanted light first.
    #[cfg(feature = "halftone")]
    fn steered<'s>(
        &self,
        segments_for: impl Fn(Version) -> &'s [Segment],
        halftone: &Halftone,
        style: &Style,
    ) -> Result<QrMatrix, EncodeError> {
        let plain = QrBuilder {
            boost_ecc: false,
            ..*self
        };
        if !halftone.steer {
            return plain.encode_with(segments_for, None);
        }
        let backdrop = style.palette.background();
        let inverted = style.inverted_ink();
        let wants = |v: Version| {
            let roles = crate::matrix::skeleton(v);
            let mut wants =
                halftone::wants(halftone, &roles, v.size() as usize, backdrop, inverted);
            for i in scene::cleared_in(style, v, &roles) {
                wants[i] = Want {
                    dark: false,
                    priority: halftone::MUST,
                };
            }
            wants
        };
        plain.encode_with(segments_for, Some(&wants))
    }

    /// Encodes text for `style`: the smallest code whose error-correction
    /// budget can afford what the style covers or clears (a logo, outlined
    /// alignment patterns). Each version is tried at the highest ECC level
    /// the text fits (when `boost_ecc` is on, the default), then the next
    /// version, up to `max_version` (SPEC §7.3). When nothing in range
    /// fits, returns the code [`encode_text`](Self::encode_text) would, and
    /// rendering it reports why.
    pub fn encode_text_for(&self, text: &str, style: &Style) -> Result<QrMatrix, EncodeError> {
        let plans = self.plans(text);
        self.fit(|v| &plans[v.count_band()], style)
    }

    /// [`encode_bytes`](Self::encode_bytes) fitted to `style`, as
    /// [`encode_text_for`](Self::encode_text_for) fits text.
    pub fn encode_bytes_for(&self, bytes: &[u8], style: &Style) -> Result<QrMatrix, EncodeError> {
        self.encode_segments_for(&[Segment::bytes(bytes)], style)
    }

    /// [`encode_segments`](Self::encode_segments) fitted to `style`, as
    /// [`encode_text_for`](Self::encode_text_for) fits text.
    pub fn encode_segments_for(
        &self,
        segments: &[Segment],
        style: &Style,
    ) -> Result<QrMatrix, EncodeError> {
        self.fit(|_| segments, style)
    }

    /// "Boost ECC, then bump the version" for a segment plan (SPEC §7.3).
    /// A picture's code is steered at each version instead of boosted.
    fn fit<'s>(
        &self,
        segments_for: impl Fn(Version) -> &'s [Segment] + Copy,
        style: &Style,
    ) -> Result<QrMatrix, EncodeError> {
        let affordable = |qr: &QrMatrix| {
            !style.check(qr).errors.iter().any(|e| {
                matches!(
                    e,
                    RenderError::LogoTooLarge { .. }
                        | RenderError::LogoOverAlignment
                        | RenderError::EccBudgetExceeded { .. }
                )
            })
        };
        let encode = |b: &QrBuilder| {
            #[cfg(feature = "halftone")]
            if let Some(h) = &style.halftone {
                return b.steered(segments_for, h, style);
            }
            b.encode_with(segments_for, None)
        };
        let first = encode(self)?;
        if affordable(&first) {
            return Ok(first);
        }
        let mut version = first.version();
        while let Some(next) = Version::new(version.number() + 1).filter(|&v| v <= self.max_version)
        {
            let qr = encode(&self.min_version(next))?;
            if affordable(&qr) {
                return Ok(qr);
            }
            version = qr.version();
        }
        Ok(first)
    }

    #[cfg(all(test, feature = "halftone"))]
    pub(crate) fn plans_for_test(&self, text: &str) -> [Vec<Segment>; 3] {
        self.plans(text)
    }

    #[cfg(all(test, feature = "halftone"))]
    pub(crate) fn encode_steered_for_test(
        &self,
        plans: &[Vec<Segment>; 3],
        wants: &[Want],
    ) -> QrMatrix {
        self.encode_with(|v| &plans[v.count_band()], Some(&|_| wants.to_vec()))
            .unwrap()
    }

    /// Encodes raw bytes in byte mode, without an ECI header.
    pub fn encode_bytes(&self, bytes: &[u8]) -> Result<QrMatrix, EncodeError> {
        self.encode_segments(&[Segment::bytes(bytes)])
    }

    /// Encodes explicit segments, in order.
    pub fn encode_segments(&self, segments: &[Segment]) -> Result<QrMatrix, EncodeError> {
        self.encode_with(|_| segments, None)
    }

    /// Picks the smallest version whose segment plan fits, then encodes it.
    /// With `steer`, the free bits are chosen for the modules it wants.
    fn encode_with<'s>(
        &self,
        segments_for: impl Fn(Version) -> &'s [Segment],
        steer: Option<&dyn Fn(Version) -> Vec<Want>>,
    ) -> Result<QrMatrix, EncodeError> {
        if self.min_version > self.max_version {
            return Err(EncodeError::InvalidVersionRange {
                min: self.min_version,
                max: self.max_version,
            });
        }

        let capacity = |v: Version, ecc: Ecc| BlockLayout::new(v, ecc).data_codewords() * 8;
        let fits = |v: Version, ecc: Ecc| {
            segment::total_bits(segments_for(v), v).is_some_and(|bits| bits <= capacity(v, ecc))
        };

        let Some(version) = Version::all()
            .filter(|&v| v >= self.min_version && v <= self.max_version)
            .find(|&v| fits(v, self.ecc))
        else {
            let v = self.max_version;
            return Err(EncodeError::DataTooLong {
                bits_needed: segment::total_bits(segments_for(v), v).unwrap_or(usize::MAX),
                bits_available: capacity(v, self.ecc),
                max_version: v,
                ecc: self.ecc,
            });
        };

        let mut ecc = self.ecc;
        if self.boost_ecc {
            while let Some(higher) = ecc.higher().filter(|&e| fits(version, e)) {
                ecc = higher;
            }
        }

        let layout = BlockLayout::new(version, ecc);
        let mut bits = BitBuffer::new();
        for s in segments_for(version) {
            s.write(&mut bits, version);
        }
        let payload_bits = bits.len();

        // Terminator, byte alignment, then alternating pad codewords.
        let capacity_bits = layout.data_codewords() * 8;
        bits.push(0, (capacity_bits - bits.len()).min(4) as u8);
        bits.push(0, ((8 - bits.len() % 8) % 8) as u8);
        for pad in [0xEC, 0x11].into_iter().cycle() {
            if bits.len() >= capacity_bits {
                break;
            }
            bits.push(pad, 8);
        }

        #[cfg_attr(not(feature = "halftone"), allow(unused_mut))]
        let mut data = bits.to_bytes();
        #[cfg_attr(not(feature = "halftone"), allow(unused_mut))]
        let mut mask = self.mask;
        #[cfg(feature = "halftone")]
        if let Some(wants) = steer {
            let free_from = (payload_bits + 4).min(capacity_bits);
            mask = Some(crate::steer::steer(
                version,
                ecc,
                &mut data,
                free_from,
                &wants(version),
                self.mask,
            ));
        }
        #[cfg(not(feature = "halftone"))]
        let _ = steer;
        let codewords = interleave_with_ecc(&data, &layout);
        Ok(QrMatrix::build(
            version,
            ecc,
            payload_bits,
            &codewords,
            layout.data_codewords(),
            mask,
        ))
    }
}

/// Splits data into blocks, appends each block's ECC, and interleaves:
/// data codewords column by column, then ECC codewords column by column.
fn interleave_with_ecc(data: &[u8], layout: &BlockLayout) -> Vec<u8> {
    let short = layout.num_short_blocks();
    let short_len = layout.short_block_data_len();
    let generator = gf::generator(layout.ecc_per_block);

    let mut blocks: Vec<(&[u8], Vec<u8>)> = Vec::with_capacity(layout.num_blocks);
    let mut rest = data;
    for i in 0..layout.num_blocks {
        let len = short_len + usize::from(i >= short);
        let (block, tail) = rest.split_at(len);
        blocks.push((block, gf::remainder(block, &generator)));
        rest = tail;
    }

    let mut out = Vec::with_capacity(layout.total_codewords);
    for i in 0..=short_len {
        for (block, _) in &blocks {
            if let Some(&b) = block.get(i) {
                out.push(b);
            }
        }
    }
    for i in 0..layout.ecc_per_block {
        for (_, ecc) in &blocks {
            out.push(ecc[i]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModuleRole;

    #[test]
    fn version_grows_with_payload() {
        let small = QrCode::encode("hi", Ecc::L).unwrap();
        assert_eq!(small.version().number(), 1);
        let text: alloc::string::String = core::iter::repeat_n('x', 500).collect();
        let big = QrCode::encode(&text, Ecc::L).unwrap();
        assert!(big.version().number() > 10);
    }

    #[test]
    fn boost_raises_ecc_without_growing() {
        let boosted = QrCode::encode("1", Ecc::L).unwrap();
        assert_eq!((boosted.version().number(), boosted.ecc()), (1, Ecc::H));
        let plain = QrCode::builder()
            .ecc(Ecc::L)
            .boost_ecc(false)
            .encode_text("1")
            .unwrap();
        assert_eq!(plain.ecc(), Ecc::L);
    }

    #[test]
    fn pinned_version_and_mask_are_honored() {
        let v = Version::new(14).unwrap();
        let m = Mask::new(5).unwrap();
        let qr = QrCode::builder()
            .version(v)
            .mask(m)
            .encode_text("https://example.com")
            .unwrap();
        assert_eq!((qr.version(), qr.mask(), qr.size()), (v, m, 73));
    }

    #[test]
    fn too_long_reports_the_budget() {
        let max = Version::new(1).unwrap();
        let err = QrCode::builder()
            .ecc(Ecc::H)
            .max_version(max)
            .encode_bytes(&[0; 20])
            .unwrap_err();
        assert_eq!(
            err,
            EncodeError::DataTooLong {
                bits_needed: 4 + 8 + 160,
                bits_available: 72,
                max_version: max,
                ecc: Ecc::H,
            }
        );
    }

    #[test]
    fn inverted_range_is_rejected() {
        let err = QrCode::builder()
            .min_version(Version::new(5).unwrap())
            .max_version(Version::new(4).unwrap())
            .encode_text("x")
            .unwrap_err();
        assert!(matches!(err, EncodeError::InvalidVersionRange { .. }));
    }

    #[test]
    fn capacity_report_counts() {
        let qr = QrCode::builder()
            .ecc(Ecc::M)
            .boost_ecc(false)
            .encode_text("01234567")
            .unwrap();
        let r = qr.capacity_report();
        assert_eq!(r.data_bits_used, 41);
        assert_eq!(r.data_bits_available, 128);
        assert_eq!(
            (r.data_codewords, r.ecc_codewords, r.total_codewords),
            (16, 10, 26)
        );
        assert_eq!(r.correctable_codewords, 4);
        assert_eq!(r.damage_tolerance_permille(), 153);
    }

    #[test]
    fn iso_annex_example_codewords_land_in_data_region() {
        // "01234567" at 1-M is the ISO 18004 Annex I example.
        let qr = QrCode::builder()
            .ecc(Ecc::M)
            .boost_ecc(false)
            .encode_text("01234567")
            .unwrap();
        let data = qr.modules().filter(|m| m.3 == ModuleRole::Data).count();
        let ecc = qr.modules().filter(|m| m.3 == ModuleRole::Ecc).count();
        assert_eq!((data, ecc), (128, 80));
    }

    #[test]
    fn output_is_deterministic() {
        let a = QrCode::encode("determinism", Ecc::Q).unwrap();
        let b = QrCode::encode("determinism", Ecc::Q).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn bytes_and_segments_are_fitted_like_text() {
        // ASCII text is planned as one byte segment without ECI, so all
        // three fit the same code, and the logo grows it.
        let style = Style::new().logo(crate::Logo::svg(r#"<svg viewBox="0 0 1 1"/>"#).unwrap());
        let b = QrCode::builder();
        let text = b.encode_text_for("https://example.com", &style).unwrap();
        let bytes = b.encode_bytes_for(b"https://example.com", &style).unwrap();
        let segments = b
            .encode_segments_for(&[Segment::bytes(b"https://example.com")], &style)
            .unwrap();
        assert_eq!(bytes, text);
        assert_eq!(segments, text);
        assert!(text.version() > b.encode_bytes(b"https://example.com").unwrap().version());
        assert!(style.check(&text).ok());

        let digits = [Segment::numeric("0123456789").unwrap()];
        let fitted = b.encode_segments_for(&digits, &style).unwrap();
        assert!(style.check(&fitted).ok());
        assert!(fitted.version() > b.encode_segments(&digits).unwrap().version());
    }

    #[cfg(feature = "halftone")]
    #[test]
    fn bytes_are_steered_for_pictures() {
        use crate::{Halftone, Rgba8Image};
        let picture = Rgba8Image::filled(8, 8, [30, 30, 30, 255]).unwrap();
        let style = Style::new().halftone(Halftone::new(picture));
        let b = QrCode::builder().min_version(Version::new(5).unwrap());
        let bytes = b.encode_bytes_for(b"HELLO WORLD", &style).unwrap();
        let steering = style.check(&bytes).steering.unwrap();
        assert!(steering.steerable_modules > 0);
        // A dark picture: steered modules lean dark.
        let plain = b.boost_ecc(false).encode_bytes(b"HELLO WORLD").unwrap();
        let dark = |qr: &QrMatrix| qr.modules().filter(|m| m.2).count();
        assert!(dark(&bytes) > dark(&plain));
    }
}
