module blink.sequence;

dataset<T> empty<T>() on cpu {
    return dataset<T>(0);
}

result<T,SequenceIndexError> get<T>(read dataset<T> data,u32 index) on cpu {
    if (index < data.length) { return Ok(data[index]); }
    return Err(SequenceIndexError(index,data.length));
}

result<void,SequenceIndexError> set<T>(inout dataset<T> data,u32 index,T value) on cpu {
    if (index < data.length) { data[index]=value; return Ok(); }
    return Err(SequenceIndexError(index,data.length));
}

result<dataset<T>,SequenceFailure<T>> push<T>(dataset<T> data,T value) on cpu {
    u32 size=data.length;
    if (size == 4294967295) {
        return Err(SequenceFailure<T>(data,SequenceError::LengthOverflow));
    }
    return Ok(resize(data,size+1,value));
}

result<SequenceRemoved<T>,SequenceFailure<T>> pop<T>(dataset<T> data) on cpu {
    u32 size=data.length;
    if (size == 0) { return Err(SequenceFailure<T>(data,SequenceError::Empty)); }
    u32 last=size-1;
    T value=data[last];
    return Ok(SequenceRemoved<T>(resize(data,last,value),value));
}

result<dataset<T>,SequenceFailure<T>> insert<T>(dataset<T> data,u32 index,T value) on cpu {
    u32 size=data.length;
    if (index > size) { return Err(SequenceFailure<T>(data,SequenceError::IndexOutOfRange)); }
    if (size == 4294967295) { return Err(SequenceFailure<T>(data,SequenceError::LengthOverflow)); }
    let output=resize(data,size+1,value);
    u32 cursor=size;
    while (cursor>index) {
        output[cursor]=output[cursor-1];
        cursor-=1;
    }
    output[index]=value;
    return Ok(output);
}

result<SequenceRemoved<T>,SequenceFailure<T>> remove<T>(dataset<T> data,u32 index) on cpu {
    u32 size=data.length;
    if (index >= size) { return Err(SequenceFailure<T>(data,SequenceError::IndexOutOfRange)); }
    T value=data[index];
    for (cursor in index..size-1) { data[cursor]=data[cursor+1]; }
    return Ok(SequenceRemoved<T>(resize(data,size-1,value),value));
}

result<dataset<T>,SequenceFailure<T>> extend<T>(dataset<T> data,read dataset<T> tail) on cpu {
    if (tail.length == 0) { return Ok(data); }
    u32 begin=data.length;
    if (tail.length > 4294967295-begin) {
        return Err(SequenceFailure<T>(data,SequenceError::LengthOverflow));
    }
    let output=resize(data,begin+tail.length,tail[0]);
    for (cursor in 0..tail.length) { output[begin+cursor]=tail[cursor]; }
    return Ok(output);
}

dataset<T> truncate<T>(dataset<T> data,u32 length) on cpu {
    if (length >= data.length) { return data; }
    T fill=data[0];
    return resize(data,length,fill);
}

dataset<T> clear<T>(dataset<T> data) on cpu {
    return truncate(data,0);
}
