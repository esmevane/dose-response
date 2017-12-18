use std::time::Duration;


/// The number of nanoseconds in a microsecond.
const NANOS_PER_MICRO: u32 = 1000;
/// The number of nanoseconds in a millisecond.
const NANOS_PER_MILLI: u32 = 1000_000;
/// The number of microseconds per second.
const MICROS_PER_SEC: u64 = 1000_000;
/// The number of milliseconds per second.
const MILLIS_PER_SEC: u64 = 1000;
/// The number of seconds in a minute.


pub fn num_milliseconds(duration: Duration) -> u64 {
    let secs_part = duration.as_secs() * MILLIS_PER_SEC;
    let nanos_part = duration.subsec_nanos() / NANOS_PER_MILLI;
    secs_part + nanos_part as u64
}


pub fn num_microseconds(duration: Duration) -> Option<u64> {
    if let Some(secs_part) = duration.as_secs().checked_mul(MICROS_PER_SEC) {
        let nanos_part = duration.subsec_nanos() / NANOS_PER_MICRO;
        return secs_part.checked_add(nanos_part as u64)
    }
    None
}