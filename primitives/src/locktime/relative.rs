// SPDX-License-Identifier: CC0-1.0

//! Provides type [`LockTime`] that implements the logic around `nSequence`/`OP_CHECKSEQUENCEVERIFY`.
//!
//! There are two types of lock time: lock-by-blockheight and lock-by-blocktime, distinguished by
//! whether bit 22 of the `u32` consensus value is set.

use core::{convert, fmt};

use crate::Sequence;
#[cfg(all(doc, feature = "alloc"))]
use crate::{relative, TxIn};

#[rustfmt::skip]                // Keep public re-exports separate.
#[doc(inline)]
pub use units::locktime::relative::{Height, Time, TimeOverflowError};

/// A relative lock time value, representing either a block height or time (512 second intervals).
///
/// Used for sequence numbers (`nSequence` in Bitcoin Core and [`TxIn::sequence`]
/// in this library) and also for the argument to opcode `OP_CHECKSEQUENCEVERIFY`.
///
/// ### Note on ordering
///
/// Locktimes may be height- or time-based, and these metrics are incommensurate; there is no total
/// ordering on locktimes. In order to compare locktimes, instead of using `<` or `>` we provide the
/// [`LockTime::is_satisfied_by`] API.
///
/// ### Relevant BIPs
///
/// * [BIP 68 Relative lock-time using consensus-enforced sequence numbers](https://github.com/bitcoin/bips/blob/master/bip-0065.mediawiki)
/// * [BIP 112 CHECKSEQUENCEVERIFY](https://github.com/bitcoin/bips/blob/master/bip-0112.mediawiki)
///
/// # Examples
///
/// ```
/// use bitcoin_primitives::relative;
/// let lock_by_height = relative::LockTime::from_height(144); // 144 blocks, approx 24h.
/// assert!(lock_by_height.is_block_height());
///
/// let lock_by_time = relative::LockTime::from_512_second_intervals(168); // 168 time intervals, approx 24h.
/// assert!(lock_by_time.is_block_time());
///
/// // Check if a lock time is satisfied by a given height or time.
/// let height = relative::Height::from(150);
/// let time = relative::Time::from_512_second_intervals(200);
/// assert!(lock_by_height.is_satisfied_by(height, time));
/// assert!(lock_by_time.is_satisfied_by(height, time));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum LockTime {
    /// A block height lock time value.
    Blocks(Height),
    /// A 512 second time interval value.
    Time(Time),
}

impl LockTime {
    /// A relative locktime of 0 is always valid, and is assumed valid for inputs that
    /// are not yet confirmed.
    pub const ZERO: LockTime = LockTime::Blocks(Height::ZERO);

    /// The number of bytes that the locktime contributes to the size of a transaction.
    pub const SIZE: usize = 4; // Serialized length of a u32.

    /// Constructs a new `LockTime` from an `nSequence` value or the argument to `OP_CHECKSEQUENCEVERIFY`.
    ///
    /// This method will **not** round-trip with [`Self::to_consensus_u32`], because relative
    /// locktimes only use some bits of the underlying `u32` value and discard the rest. If
    /// you want to preserve the full value, you should use the [`Sequence`] type instead.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bitcoin_primitives::relative;
    ///
    /// // Values with bit 22 set to 0 will be interpreted as height-based lock times.
    /// let height: u32 = 144; // 144 blocks, approx 24h.
    /// let lock_time = relative::LockTime::from_consensus(height)?;
    /// assert!(lock_time.is_block_height());
    /// assert_eq!(lock_time.to_consensus_u32(), height);
    ///
    /// // Values with bit 22 set to 1 will be interpreted as time-based lock times.
    /// let time: u32 = 168 | (1 << 22) ; // Bit 22 is 1 with time approx 24h.
    /// let lock_time = relative::LockTime::from_consensus(time)?;
    /// assert!(lock_time.is_block_time());
    /// assert_eq!(lock_time.to_consensus_u32(), time);
    ///
    /// # Ok::<_, bitcoin_primitives::relative::DisabledLockTimeError>(())
    /// ```
    #[inline]
    pub fn from_consensus(n: u32) -> Result<Self, DisabledLockTimeError> {
        let sequence = crate::Sequence::from_consensus(n);
        sequence.to_relative_lock_time().ok_or(DisabledLockTimeError(n))
    }

    /// Returns the `u32` value used to encode this locktime in an `nSequence` field or
    /// argument to `OP_CHECKSEQUENCEVERIFY`.
    ///
    /// # Warning
    ///
    /// Locktimes are not ordered by the natural ordering on `u32`. If you want to
    /// compare locktimes, use [`Self::is_implied_by`] or similar methods.
    #[inline]
    pub fn to_consensus_u32(self) -> u32 {
        match self {
            LockTime::Blocks(ref h) => h.to_consensus_u32(),
            LockTime::Time(ref t) => t.to_consensus_u32(),
        }
    }

    /// Constructs a new `LockTime` from the sequence number of a Bitcoin input.
    ///
    /// This method will **not** round-trip with [`Self::to_sequence`]. See the
    /// docs for [`Self::from_consensus`] for more information.
    /// # Examples
    ///
    /// ```rust
    /// # use bitcoin_primitives::{Sequence, relative};
    ///
    /// // Interpret a sequence number from a Bitcoin transaction input as a relative lock time
    /// let sequence_number = Sequence::from_consensus(144); // 144 blocks, approx 24h.
    /// let lock_time = relative::LockTime::from_sequence(sequence_number)?;
    /// assert!(lock_time.is_block_height());
    ///
    /// # Ok::<_, bitcoin_primitives::relative::DisabledLockTimeError>(())
    /// ```
    #[inline]
    pub fn from_sequence(n: Sequence) -> Result<Self, DisabledLockTimeError> {
        Self::from_consensus(n.to_consensus_u32())
    }

    /// Encodes the locktime as a sequence number.
    #[inline]
    pub fn to_sequence(self) -> Sequence { Sequence::from_consensus(self.to_consensus_u32()) }

    /// Constructs a new `LockTime` from `n`, expecting `n` to be a 16-bit count of blocks.
    #[inline]
    pub const fn from_height(n: u16) -> Self { LockTime::Blocks(Height::from_height(n)) }

    /// Constructs a new `LockTime` from `n`, expecting `n` to be a count of 512-second intervals.
    ///
    /// This function is a little awkward to use, and users may wish to instead use
    /// [`Self::from_seconds_floor`] or [`Self::from_seconds_ceil`].
    #[inline]
    pub const fn from_512_second_intervals(intervals: u16) -> Self {
        LockTime::Time(Time::from_512_second_intervals(intervals))
    }

    /// Construct a new [`LockTime`] from seconds, converting the seconds into 512 second interval
    /// with truncating division.
    ///
    /// # Errors
    ///
    /// Will return an error if the input cannot be encoded in 16 bits.
    #[inline]
    pub const fn from_seconds_floor(seconds: u32) -> Result<Self, TimeOverflowError> {
        match Time::from_seconds_floor(seconds) {
            Ok(time) => Ok(LockTime::Time(time)),
            Err(e) => Err(e),
        }
    }

    /// Construct a new [`LockTime`] from seconds, converting the seconds into 512 second interval
    /// with ceiling division.
    ///
    /// # Errors
    ///
    /// Will return an error if the input cannot be encoded in 16 bits.
    #[inline]
    pub const fn from_seconds_ceil(seconds: u32) -> Result<Self, TimeOverflowError> {
        match Time::from_seconds_ceil(seconds) {
            Ok(time) => Ok(LockTime::Time(time)),
            Err(e) => Err(e),
        }
    }

    /// Returns true if both lock times use the same unit i.e., both height based or both time based.
    #[inline]
    pub const fn is_same_unit(self, other: LockTime) -> bool {
        matches!(
            (self, other),
            (LockTime::Blocks(_), LockTime::Blocks(_)) | (LockTime::Time(_), LockTime::Time(_))
        )
    }

    /// Returns true if this lock time value is in units of block height.
    #[inline]
    pub const fn is_block_height(self) -> bool { matches!(self, LockTime::Blocks(_)) }

    /// Returns true if this lock time value is in units of time.
    #[inline]
    pub const fn is_block_time(self) -> bool { !self.is_block_height() }

    /// Returns true if this [`relative::LockTime`] is satisfied by either height or time.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bitcoin_primitives::Sequence;
    /// # use bitcoin_primitives::relative;
    ///
    /// # let required_height = 100;       // 100 blocks.
    /// # let intervals = 70;     // Approx 10 hours.
    /// # let current_height = || relative::Height::from(required_height + 10);
    /// # let current_time = || relative::Time::from_512_second_intervals(intervals + 10);
    /// # let lock = Sequence::from_height(required_height).to_relative_lock_time().expect("valid height");
    ///
    /// // Users that have chain data can get the current height and time to check against a lock.
    /// assert!(lock.is_satisfied_by(current_height(), current_time()));
    /// ```
    #[inline]
    pub fn is_satisfied_by(self, h: Height, t: Time) -> bool {
        if let Ok(true) = self.is_satisfied_by_height(h) {
            true
        } else {
            matches!(self.is_satisfied_by_time(t), Ok(true))
        }
    }

    /// Returns true if satisfaction of `other` lock time implies satisfaction of this
    /// [`relative::LockTime`].
    ///
    /// A lock time can only be satisfied by n blocks being mined or n seconds passing. If you have
    /// two lock times (same unit) then the larger lock time being satisfied implies (in a
    /// mathematical sense) the smaller one being satisfied.
    ///
    /// This function is useful when checking sequence values against a lock, first one checks the
    /// sequence represents a relative lock time by converting to `LockTime` then use this function
    /// to see if satisfaction of the newly created lock time would imply satisfaction of `self`.
    ///
    /// Can also be used to remove the smaller value of two `OP_CHECKSEQUENCEVERIFY` operations
    /// within one branch of the script.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bitcoin_primitives::Sequence;
    ///
    /// # let required_height = 100;       // 100 blocks.
    /// # let lock = Sequence::from_height(required_height).to_relative_lock_time().expect("valid height");
    /// # let test_sequence = Sequence::from_height(required_height + 10);
    ///
    /// let satisfied = match test_sequence.to_relative_lock_time() {
    ///     None => false, // Handle non-lock-time case.
    ///     Some(test_lock) => lock.is_implied_by(test_lock),
    /// };
    /// assert!(satisfied);
    /// ```
    #[inline]
    pub fn is_implied_by(self, other: LockTime) -> bool {
        use LockTime as L;

        match (self, other) {
            (L::Blocks(this), L::Blocks(other)) => this.value() <= other.value(),
            (L::Time(this), L::Time(other)) => this.value() <= other.value(),
            _ => false, // Not the same units.
        }
    }

    /// Returns true if satisfaction of the sequence number implies satisfaction of this lock time.
    ///
    /// When deciding whether an instance of `<n> CHECKSEQUENCEVERIFY` will pass, this
    /// method can be used by parsing `n` as a [`LockTime`] and calling this method
    /// with the sequence number of the input which spends the script.
    ///
    /// # Examples
    ///
    /// ```
    /// # use bitcoin_primitives::{Sequence, relative};
    ///
    /// let sequence = Sequence::from_consensus(1 << 22 | 168); // Bit 22 is 1 with time approx 24h.
    /// let lock_time = relative::LockTime::from_sequence(sequence)?;
    /// let input_sequence = Sequence::from_consensus(1 << 22 | 336); // Approx 48h.
    /// assert!(lock_time.is_block_time());
    ///
    /// assert!(lock_time.is_implied_by_sequence(input_sequence));
    ///
    /// # Ok::<_, bitcoin_primitives::relative::DisabledLockTimeError>(())
    /// ```
    #[inline]
    pub fn is_implied_by_sequence(self, other: Sequence) -> bool {
        if let Ok(other) = LockTime::from_sequence(other) {
            self.is_implied_by(other)
        } else {
            false
        }
    }

    /// Returns true if this [`relative::LockTime`] is satisfied by [`Height`].
    ///
    /// # Errors
    ///
    /// Returns an error if this lock is not lock-by-height.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bitcoin_primitives::Sequence;
    /// # use bitcoin_primitives::relative;
    ///
    /// let required_height: u16 = 100;
    /// let lock = Sequence::from_height(required_height).to_relative_lock_time().expect("valid height");
    /// assert!(lock.is_satisfied_by_height(relative::Height::from(required_height + 1)).expect("a height"));
    /// ```
    #[inline]
    pub fn is_satisfied_by_height(self, height: Height) -> Result<bool, IncompatibleHeightError> {
        use LockTime as L;

        match self {
            L::Blocks(ref required_height) => Ok(required_height.value() <= height.value()),
            L::Time(time) => Err(IncompatibleHeightError { height, time }),
        }
    }

    /// Returns true if this [`relative::LockTime`] is satisfied by [`Time`].
    ///
    /// # Errors
    ///
    /// Returns an error if this lock is not lock-by-time.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use bitcoin_primitives::Sequence;
    /// # use bitcoin_primitives::relative;
    ///
    /// let intervals: u16 = 70; // approx 10 hours;
    /// let lock = Sequence::from_512_second_intervals(intervals).to_relative_lock_time().expect("valid time");
    /// assert!(lock.is_satisfied_by_time(relative::Time::from_512_second_intervals(intervals + 10)).expect("a time"));
    /// ```
    #[inline]
    pub fn is_satisfied_by_time(self, time: Time) -> Result<bool, IncompatibleTimeError> {
        use LockTime as L;

        match self {
            L::Time(ref t) => Ok(t.value() <= time.value()),
            L::Blocks(height) => Err(IncompatibleTimeError { time, height }),
        }
    }
}

impl From<Height> for LockTime {
    #[inline]
    fn from(h: Height) -> Self { LockTime::Blocks(h) }
}

impl From<Time> for LockTime {
    #[inline]
    fn from(t: Time) -> Self { LockTime::Time(t) }
}

impl fmt::Display for LockTime {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use LockTime as L;

        if f.alternate() {
            match *self {
                L::Blocks(ref h) => write!(f, "block-height {}", h),
                L::Time(ref t) => write!(f, "block-time {} (512 second intervals)", t),
            }
        } else {
            match *self {
                L::Blocks(ref h) => fmt::Display::fmt(h, f),
                L::Time(ref t) => fmt::Display::fmt(t, f),
            }
        }
    }
}

impl convert::TryFrom<Sequence> for LockTime {
    type Error = DisabledLockTimeError;
    #[inline]
    fn try_from(seq: Sequence) -> Result<LockTime, DisabledLockTimeError> {
        LockTime::from_sequence(seq)
    }
}

impl From<LockTime> for Sequence {
    #[inline]
    fn from(lt: LockTime) -> Sequence { lt.to_sequence() }
}

/// Error returned when a sequence number is parsed as a lock time, but its
/// "disable" flag is set.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DisabledLockTimeError(u32);

impl DisabledLockTimeError {
    /// Accessor for the `u32` whose "disable" flag was set, preventing
    /// it from being parsed as a relative locktime.
    #[inline]
    pub fn disabled_locktime_value(&self) -> u32 { self.0 }
}

impl fmt::Display for DisabledLockTimeError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "lock time 0x{:08x} has disable flag set", self.0)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for DisabledLockTimeError {}

/// Tried to satisfy a lock-by-blocktime lock using a height value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncompatibleHeightError {
    /// Attempted to satisfy a lock-by-blocktime lock with this height.
    height: Height,
    /// The inner time value of the lock-by-blocktime lock.
    time: Time,
}

impl IncompatibleHeightError {
    /// Returns the height that was erroneously used to try and satisfy a lock-by-blocktime lock.
    pub fn incompatible(&self) -> Height { self.height }

    /// Returns the time value of the lock-by-blocktime lock.
    pub fn expected(&self) -> Time { self.time }
}

impl fmt::Display for IncompatibleHeightError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "tried to satisfy a lock-by-blocktime lock {} with height: {}",
            self.time, self.height
        )
    }
}

#[cfg(feature = "std")]
impl std::error::Error for IncompatibleHeightError {}

/// Tried to satisfy a lock-by-blockheight lock using a time value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncompatibleTimeError {
    /// Attempted to satisfy a lock-by-blockheight lock with this time.
    time: Time,
    /// The inner height value of the lock-by-blockheight lock.
    height: Height,
}

impl IncompatibleTimeError {
    /// Returns the time that was erroneously used to try and satisfy a lock-by-blockheight lock.
    pub fn incompatible(&self) -> Time { self.time }

    /// Returns the height value of the lock-by-blockheight lock.
    pub fn expected(&self) -> Height { self.height }
}

impl fmt::Display for IncompatibleTimeError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "tried to satisfy a lock-by-blockheight lock {} with time: {}",
            self.height, self.time
        )
    }
}

#[cfg(feature = "std")]
impl std::error::Error for IncompatibleTimeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_alternate() {
        let lock_by_height = LockTime::from_height(10);
        let lock_by_time = LockTime::from_512_second_intervals(70);

        assert_eq!(format!("{}", lock_by_height), "10");
        assert_eq!(format!("{:#}", lock_by_height), "block-height 10");
        assert!(!format!("{:?}", lock_by_height).is_empty());

        assert_eq!(format!("{}", lock_by_time), "70");
        assert_eq!(format!("{:#}", lock_by_time), "block-time 70 (512 second intervals)");
        assert!(!format!("{:?}", lock_by_time).is_empty());
    }

    #[test]
    fn from_seconds_ceil_and_floor() {
        let time = 70 * 512 + 1;
        let lock_by_time = LockTime::from_seconds_ceil(time).unwrap();
        assert_eq!(lock_by_time, LockTime::from_512_second_intervals(71));

        let lock_by_time = LockTime::from_seconds_floor(time).unwrap();
        assert_eq!(lock_by_time, LockTime::from_512_second_intervals(70));

        let mut max_time = 0xffff * 512;
        assert_eq!(LockTime::from_seconds_ceil(max_time), LockTime::from_seconds_floor(max_time));
        max_time += 512;
        assert!(LockTime::from_seconds_ceil(max_time).is_err());
        assert!(LockTime::from_seconds_floor(max_time).is_err());
    }

    #[test]
    fn parses_correctly_to_height_or_time() {
        let height1 = Height::from(10);
        let height2 = Height::from(11);
        let time1 = Time::from_512_second_intervals(70);
        let time2 = Time::from_512_second_intervals(71);

        let lock_by_height1 = LockTime::from(height1);
        let lock_by_height2 = LockTime::from(height2);
        let lock_by_time1 = LockTime::from(time1);
        let lock_by_time2 = LockTime::from(time2);

        assert!(lock_by_height1.is_block_height());
        assert!(!lock_by_height1.is_block_time());

        assert!(!lock_by_time1.is_block_height());
        assert!(lock_by_time1.is_block_time());

        // Test is_same_unit() logic
        assert!(lock_by_height1.is_same_unit(lock_by_height2));
        assert!(!lock_by_height1.is_same_unit(lock_by_time1));
        assert!(lock_by_time1.is_same_unit(lock_by_time2));
        assert!(!lock_by_time1.is_same_unit(lock_by_height1));
    }

    #[test]
    fn satisfied_by_height() {
        let height = Height::from(10);
        let time = Time::from_512_second_intervals(70);

        let lock_by_height = LockTime::from(height);

        assert!(!lock_by_height.is_satisfied_by(Height::from(9), time));
        assert!(lock_by_height.is_satisfied_by(Height::from(10), time));
        assert!(lock_by_height.is_satisfied_by(Height::from(11), time));
    }

    #[test]
    fn satisfied_by_time() {
        let height = Height::from(10);
        let time = Time::from_512_second_intervals(70);

        let lock_by_time = LockTime::from(time);

        assert!(!lock_by_time.is_satisfied_by(height, Time::from_512_second_intervals(69)));
        assert!(lock_by_time.is_satisfied_by(height, Time::from_512_second_intervals(70)));
        assert!(lock_by_time.is_satisfied_by(height, Time::from_512_second_intervals(71)));
    }

    #[test]
    fn height_correctly_implies() {
        let height = Height::from(10);
        let lock_by_height = LockTime::from(height);

        assert!(!lock_by_height.is_implied_by(LockTime::from(Height::from(9))));
        assert!(lock_by_height.is_implied_by(LockTime::from(Height::from(10))));
        assert!(lock_by_height.is_implied_by(LockTime::from(Height::from(11))));
    }

    #[test]
    fn time_correctly_implies() {
        let time = Time::from_512_second_intervals(70);
        let lock_by_time = LockTime::from(time);

        assert!(!lock_by_time.is_implied_by(LockTime::from(Time::from_512_second_intervals(69))));
        assert!(lock_by_time.is_implied_by(LockTime::from(Time::from_512_second_intervals(70))));
        assert!(lock_by_time.is_implied_by(LockTime::from(Time::from_512_second_intervals(71))));
    }

    #[test]
    fn sequence_correctly_implies() {
        let height = Height::from(10);
        let time = Time::from_512_second_intervals(70);

        let lock_by_height = LockTime::from(height);
        let lock_by_time = LockTime::from(time);

        let seq_height = Sequence::from(lock_by_height);
        let seq_time = Sequence::from(lock_by_time);

        assert!(lock_by_height.is_implied_by_sequence(seq_height));
        assert!(!lock_by_height.is_implied_by_sequence(seq_time));

        assert!(lock_by_time.is_implied_by_sequence(seq_time));
        assert!(!lock_by_time.is_implied_by_sequence(seq_height));

        let disabled_sequence = Sequence::from_consensus(1 << 31);
        assert!(!lock_by_height.is_implied_by_sequence(disabled_sequence));
        assert!(!lock_by_time.is_implied_by_sequence(disabled_sequence));
    }

    #[test]
    fn incorrect_units_do_not_imply() {
        let time = Time::from_512_second_intervals(70);
        let height = Height::from(10);

        let lock_by_time = LockTime::from(time);
        assert!(!lock_by_time.is_implied_by(LockTime::from(height)));
    }

    #[test]
    fn consensus_round_trip() {
        assert!(LockTime::from_consensus(1 << 31).is_err());
        assert!(LockTime::from_consensus(1 << 30).is_ok());
        // Relative locktimes do not care about bits 17 through 21.
        assert_eq!(LockTime::from_consensus(65536), LockTime::from_consensus(0));

        for val in [0u32, 1, 1000, 65535] {
            let seq = Sequence::from_consensus(val);
            let lt = LockTime::from_consensus(val).unwrap();
            assert_eq!(lt.to_consensus_u32(), val);
            assert_eq!(lt.to_sequence(), seq);
            assert_eq!(LockTime::from_sequence(seq).unwrap().to_sequence(), seq);

            let seq = Sequence::from_consensus(val + (1 << 22));
            let lt = LockTime::from_consensus(val + (1 << 22)).unwrap();
            assert_eq!(lt.to_consensus_u32(), val + (1 << 22));
            assert_eq!(lt.to_sequence(), seq);
            assert_eq!(LockTime::from_sequence(seq).unwrap().to_sequence(), seq);
        }
    }

    #[test]
    fn disabled_locktime_error() {
        let disabled_sequence = Sequence::from_consensus(1 << 31);
        let err = LockTime::try_from(disabled_sequence).unwrap_err();

        assert_eq!(err.disabled_locktime_value(), 1 << 31);
        assert!(!format!("{}", err).is_empty());
    }

    #[test]
    fn incompatible_height_error() {
        let height = Height::from(10);
        let time = Time::from_512_second_intervals(70);
        let lock_by_time = LockTime::from(time);
        let err = lock_by_time.is_satisfied_by_height(height).unwrap_err();

        assert_eq!(err.incompatible(), height);
        assert_eq!(err.expected(), time);
        assert!(!format!("{}", err).is_empty());
    }

    #[test]
    fn incompatible_time_error() {
        let height = Height::from(10);
        let time = Time::from_512_second_intervals(70);
        let lock_by_height = LockTime::from(height);
        let err = lock_by_height.is_satisfied_by_time(time).unwrap_err();

        assert_eq!(err.incompatible(), time);
        assert_eq!(err.expected(), height);
        assert!(!format!("{}", err).is_empty());
    }
}
