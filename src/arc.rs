use core::str;
use std::{mem::transmute, ptr, sync::Arc};

pub const MAX_INLINE: usize = 12;

/// An owned Atomically reference counted Umbra-style string
pub struct UmbraArcString {
    len: u32,
    prefix: [u8; 4],
    extra: UmbraArcExtra,
}

pub union UmbraArcExtra {
    data: [u8; 8],
    ptr: *const str,
}

impl UmbraArcString {
    pub fn new(val: impl AsRef<str>) -> UmbraArcString {
        let val_str = val.as_ref();

        // TODO: should I check for overflow here?
        let len = val_str.len();

        if len <= MAX_INLINE {
            let mut inline: [u8; 12] = [0; 12];
            inline[..len].copy_from_slice(val_str.as_bytes());
            // SAFETY: inline is of length 12 and align 1, and it is being split into arrays of length 4 and 8
            let (prefix, extra) = unsafe { transmute(inline) };

            UmbraArcString {
                len: len as u32,
                prefix,
                extra: UmbraArcExtra { data: extra },
            }
        } else {
            let mut prefix = [0; 4];
            prefix.copy_from_slice(&val_str.as_bytes()[0..4]);
            let stored: Arc<str> = Arc::from(val_str);
            let ptr = Arc::into_raw(stored);

            UmbraArcString {
                len: len as u32,
                prefix,
                extra: UmbraArcExtra { ptr },
            }
        }
    }

    pub fn is_inline(&self) -> bool {
        self.len <= MAX_INLINE as u32
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }
}

impl UmbraArcString {
    #[inline]
    fn suffix_bytes(&self) -> &[u8] {
        if self.is_inline() {
            // SAFETY: is_inline() so data is valid
            unsafe { &self.extra.data }
        } else {
            // SAFETY: is_inline() so ptr is valid
            let s = unsafe { &*self.extra.ptr };
            &s.as_bytes()[4..]
        }
    }
}

impl Clone for UmbraArcString {
    fn clone(&self) -> Self {
        if self.is_inline() {
            Self {
                len: self.len.clone(),
                prefix: self.prefix.clone(),
                // SAFETY: is_inline() so data is active
                extra: unsafe { self.extra.inner_data_clone() },
            }
        } else {
            Self {
                len: self.len.clone(),
                prefix: self.prefix.clone(),
                // SAFETY: !is_inline() so ptr is active
                extra: unsafe { self.extra.inner_ptr_clone() },
            }
        }
    }
}

impl AsRef<str> for UmbraArcString {
    fn as_ref(&self) -> &str {
        if self.is_inline() {
            // SAFETY: following 8 bytes are extra and data is active as is_inline()
            let byte_arr: &[u8; 12] = unsafe { transmute(&self.prefix) };
            // SAFETY: bytes were taken from str::as_bytes, so should be valid utf-8
            unsafe { str::from_utf8_unchecked(byte_arr) }
        } else {
            // SAFETY: !is_inline() so ptr is active
            unsafe { &*self.extra.ptr }
        }
    }
}

impl Eq for UmbraArcString {}

impl PartialEq<UmbraArcString> for UmbraArcString {
    fn eq(&self, other: &UmbraArcString) -> bool {
        let self_len_prefix = ptr::from_ref(self).cast::<u64>();
        let other_len_prefix = ptr::from_ref(other).cast::<u64>();
        // SAFETY: both are valid references and UmbraArcString has 8byte alignment so the reads are aligned
        if unsafe { *self_len_prefix != *other_len_prefix } {
            return false;
        }

        if self.is_inline() && other.is_inline() {
            // SAFETY: both are inline so data is active
            unsafe { self.extra.data == other.extra.data }
        } else {
            self.suffix_bytes() == self.suffix_bytes()
        }
    }
}

impl PartialEq<&str> for UmbraArcString {
    fn eq(&self, other: &&str) -> bool {
        self.as_ref() == *other
    }
}

impl Ord for UmbraArcString {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.prefix.cmp(&other.prefix) {
            std::cmp::Ordering::Less => std::cmp::Ordering::Less,
            std::cmp::Ordering::Equal => {
                if self.len <= 4 && other.len <= 4 {
                    std::cmp::Ordering::Equal
                } else if self.is_inline() && other.is_inline() {
                    let ordering = unsafe { self.extra.data.cmp(&other.extra.data) };
                    ordering.then_with(|| self.len.cmp(&other.len))
                } else {
                    self.suffix_bytes().cmp(other.suffix_bytes())
                }
            }
            std::cmp::Ordering::Greater => std::cmp::Ordering::Greater,
        }
    }
}

impl PartialOrd<UmbraArcString> for UmbraArcString {
    fn partial_cmp(&self, other: &UmbraArcString) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialOrd<&str> for UmbraArcString {
    fn partial_cmp(&self, other: &&str) -> Option<std::cmp::Ordering> {
        Some(self.as_ref().cmp(other))
    }
}

impl Drop for UmbraArcString {
    fn drop(&mut self) {
        if !self.is_inline() {
            // SAFETY: !is_inline() so ptr is active, ptr is private and created with Arc::into_raw
            unsafe { self.extra.inner_ptr_drop() }
        }
    }
}

impl UmbraArcExtra {
    /// SAFETY: Must be called with ptr field active and it containing a pointer from Arc::into_raw
    unsafe fn inner_ptr_clone(&self) -> Self {
        // SAFETY: ptr must be active under preconditions
        let arc_raw = unsafe { self.ptr };

        // SAFETY: ptr must have a pointer from Arc::into_raw
        let old_arc = unsafe { Arc::from_raw(arc_raw) };
        let new_arc = old_arc.clone();

        // prevent dropping of old from decrementing ref count
        let _ = Arc::into_raw(old_arc);

        UmbraArcExtra {
            ptr: Arc::into_raw(new_arc),
        }
    }

    /// SAFETY: Must be called with data field active
    unsafe fn inner_data_clone(&self) -> Self {
        UmbraArcExtra {
            // SAFETY: data must be active under preconditions
            data: unsafe { self.data.clone() },
        }
    }

    /// SAFETY: Must be called with ptr field active and it containing a pointer from Arc::into_raw
    unsafe fn inner_ptr_drop(&self) {
        // SAFETY: ptr must be active under preconditions
        let arc_raw = unsafe { self.ptr };

        // SAFETY: ptr must have a pointer from Arc::into_raw
        let _ = unsafe { Arc::from_raw(arc_raw) };
    }
}
