//! Kani proof of the memory ceiling (ADR 0014): the bytes a call holds in all its memories and tables, summed, never
//! go over the ceiling.

use super::*;

/// Over any three growths of memories or tables, from any sizes, under any ceiling: the count never exceeds the
/// ceiling and never wraps; an allowed growth adds exactly its bytes; a refused one adds nothing and is remembered.
#[kani::proof]
#[kani::unwind(4)]
fn the_count_never_goes_over_the_ceiling() {
    let mut limiter = MemoryLimiter::new(ByteSize::new(kani::any()));
    for _ in 0..3 {
        let before = limiter.used_bytes;
        let was_denied = limiter.denied;
        let current: usize = kani::any();
        let desired: usize = kani::any();
        let table: bool = kani::any();
        let (allowed, bytes) = if table {
            let element = size_of::<usize>();
            let bytes = desired
                .saturating_mul(element)
                .saturating_sub(current.saturating_mul(element));
            (limiter.table_growing(current, desired, None), bytes)
        } else {
            (
                limiter.memory_growing(current, desired, None),
                desired.saturating_sub(current),
            )
        };
        assert!(limiter.used_bytes <= limiter.max_bytes);
        match allowed {
            Ok(true) => {
                // Exact, unless the ceiling is the whole address space, where saturation is no ceiling at all.
                assert!(
                    before.checked_add(bytes) == Some(limiter.used_bytes)
                        || limiter.max_bytes == usize::MAX
                );
                assert_eq!(limiter.denied, was_denied);
            }
            Ok(false) => {
                assert_eq!(limiter.used_bytes, before);
                assert!(limiter.denied);
            }
            Err(_) => panic!("the limiter never fails"),
        }
    }
}
