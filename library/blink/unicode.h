module blink.unicode;

enum DecodeReason : u32 {
    EndOfInput = 1;
    Truncated = 2;
    InvalidLead = 3;
    InvalidContinuation = 4;
    InvalidSequence = 5;
    InvalidSurrogate = 6;
};
struct DecodeError { u32 index; DecodeReason reason; };
struct DecodedScalar { u32 value; u32 next; };
enum Utf8Encoding : u32 {
    One(u8) = 1;
    Two(array<u8,2>) = 2;
    Three(array<u8,3>) = 3;
    Four(array<u8,4>) = 4;
};
enum Utf16Encoding : u32 {
    One(u16) = 1;
    Two(array<u16,2>) = 2;
};

result<DecodedScalar,DecodeError> decode_utf8(read dataset<u8> text,u32 offset) on cpu;
result<DecodedScalar,DecodeError> decode_utf16(read dataset<u16> text,u32 offset) on cpu;
result<Utf8Encoding,u32> encode_utf8(u32 value) on cpu;
result<Utf16Encoding,u32> encode_utf16(u32 value) on cpu;
