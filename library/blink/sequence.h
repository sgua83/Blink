module blink.sequence;

enum SequenceError : u32 {
    Empty = 1;
    IndexOutOfRange = 2;
    LengthOverflow = 3;
};

struct SequenceIndexError { u32 index; u32 length; };
struct SequenceFailure<T> { dataset<T> data; SequenceError reason; };
struct SequenceRemoved<T> { dataset<T> data; T value; };

dataset<T> empty<T>() on cpu;
result<T,SequenceIndexError> get<T>(read dataset<T> data,u32 index) on cpu;
result<void,SequenceIndexError> set<T>(inout dataset<T> data,u32 index,T value) on cpu;
result<dataset<T>,SequenceFailure<T>> push<T>(dataset<T> data,T value) on cpu;
result<SequenceRemoved<T>,SequenceFailure<T>> pop<T>(dataset<T> data) on cpu;
result<dataset<T>,SequenceFailure<T>> insert<T>(dataset<T> data,u32 index,T value) on cpu;
result<SequenceRemoved<T>,SequenceFailure<T>> remove<T>(dataset<T> data,u32 index) on cpu;
result<dataset<T>,SequenceFailure<T>> extend<T>(dataset<T> data,read dataset<T> tail) on cpu;
dataset<T> truncate<T>(dataset<T> data,u32 length) on cpu;
dataset<T> clear<T>(dataset<T> data) on cpu;
