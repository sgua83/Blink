module blink.unicode_scalar;
// Pure value facts shared by the Unicode API and source-text decoding.
// The bit layout preserves the original scalar encoder; valid Unicode units
// are consumed after the caller's is_scalar check.
struct Utf8Units {u32 length;u32 bits;};
bool is_scalar(u32 value) on cpu;
Utf8Units utf8_units(u32 value) on cpu;
