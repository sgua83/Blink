module bootstrap.names;
extern "C" bool dependency_active(address path, u64 count, NameBytes key) on cpu {
    u64 at = count;
    while (at != 0u64) {
        at -= 1u64;
        if (equal(import_at(path, at), key)) { return true; }
    }
    return false;
}
NameBytes empty() on cpu {return NameBytes(unsafe_address(0u64),0u64);}
u8 byte_at(NameBytes value,u64 at) on cpu {return unsafe_position<u8>(unsafe_address(address_bits(value.bytes)+at));}
NameBytes import_at(address imports,u64 at) on cpu {return unsafe_position<NameBytes>(row_at<NameBytes>(imports,at));}
bool equal(NameBytes a,NameBytes b) on cpu {
    if(a.length!=b.length){return false;}
    u64 at=0u64;while(at<a.length){if(byte_at(a,at)!=byte_at(b,at)){return false;}at+=1u64;}return true;
}
bool qualified(NameBytes name) on cpu {
    u64 at=1u64;while(at<name.length){if(byte_at(name,at-1u64)==58&&byte_at(name,at)==58){return true;}at+=1u64;}return false;
}
bool visible(NameHit hit,NameBytes module,address imports,u64 count) on cpu {
    if(hit.present==0u64){return false;}
    if(equal(hit.owner,module)){return true;}
    if(hit.public==0u64){return false;}
    u64 at=0u64;while(at<count){if(equal(import_at(imports,at),hit.owner)){return true;}at+=1u64;}return false;
}
NameBytes qualify(NameBytes module,NameBytes name,address candidate) on cpu {
    u64 at=0u64;while(at<module.length){unsafe_position<u8>(unsafe_address(address_bits(candidate)+at))=byte_at(module,at);at+=1u64;}
    unsafe_position<u8>(unsafe_address(address_bits(candidate)+module.length))=u8(58);
    unsafe_position<u8>(unsafe_address(address_bits(candidate)+module.length+1u64))=u8(58);
    at=0u64;while(at<name.length){unsafe_position<u8>(unsafe_address(address_bits(candidate)+module.length+2u64+at))=byte_at(name,at);at+=1u64;}
    return NameBytes(candidate,module.length+2u64+name.length);
}
extern "C" NameResult resolve(NameBytes name,NameBytes module,address imports,u64 count,
    address candidate,address context,cfn<NameHit,address,NameBytes> lookup) on cpu requires nonzero(lookup) {
    if(qualified(name)){NameHit hit=lookup(context,name);if(visible(hit,module,imports,count)){return NameResult(hit.name,1u64);}}
    NameHit local=lookup(context,qualify(module,name,candidate));
    if(visible(local,module,imports,count)){return NameResult(local.name,1u64);}
    NameBytes first=empty();u64 found=0u64;u64 at=0u64;
    while(at<count){
        NameHit hit=lookup(context,qualify(import_at(imports,at),name,candidate));
        if(visible(hit,module,imports,count)){
            if(found==0u64){first=hit.name;found=1u64;}else{if(!equal(first,hit.name)){return NameResult(empty(),2u64);}}
        }
        at+=1u64;
    }
    return NameResult(first,found);
}

address row_at<T>(address rows, u64 index) on cpu {
    return unsafe_address(address_bits(rows) + index * size_of<T>());
}

void sift_rows<T>(address rows, u64 start, u64 count, address context,
    cfn<bool,address,address,address> less) on cpu requires nonzero(less) {
    T value = unsafe_position<T>(row_at<T>(rows, start));
    u64 root = start;
    while (root < count / 2u64) {
        u64 child = 2u64 * root + 1u64;
        if (child + 1u64 < count && less(row_at<T>(rows, child), row_at<T>(rows, child + 1u64), context)) {
            child += 1u64;
        }
        address next = row_at<T>(rows, child);
        if (!less(address_of(value), next, context)) { break; }
        unsafe_position<T>(row_at<T>(rows, root)) = unsafe_position<T>(next);
        root = child;
    }
    unsafe_position<T>(row_at<T>(rows, root)) = value;
}

void sort_rows<T>(address rows, u64 count, address context,
    cfn<bool,address,address,address> less) on cpu requires nonzero(less) {
    u64 root = count / 2u64;
    while (root != 0u64) {
        root -= 1u64;
        sift_rows<T>(rows, root, count, context, less);
    }
    u64 end = count;
    while (end > 1u64) {
        end -= 1u64;
        address last = row_at<T>(rows, end);
        T first = unsafe_position<T>(rows);
        unsafe_position<T>(rows) = unsafe_position<T>(last);
        unsafe_position<T>(last) = first;
        sift_rows<T>(rows, 0u64, end, context, less);
    }
}

i32 compare_name(NameBytes left, NameBytes right) on cpu {
    u64 at = 0u64;
    while (at < left.length && at < right.length) {
        u8 a = byte_at(left, at);
        u8 b = byte_at(right, at);
        if (a < b) { return i32(-1); }
        if (a > b) { return i32(1); }
        at += 1u64;
    }
    if (left.length < right.length) { return i32(-1); }
    if (left.length > right.length) { return i32(1); }
    return i32(0);
}

extern "C" bool name_before(address left, address right, address context) on cpu {
    return compare_name(unsafe_position<NameBytes>(left), unsafe_position<NameBytes>(right)) < i32(0);
}

extern "C" u64 order_imports(address names, u64 count) on cpu {
    sort_rows<NameBytes>(names, count, address(), c_function(name_before));

    u64 unique = 0u64;
    u64 at = 0u64;
    NameBytes previous = empty();
    while (at < count) {
        NameBytes name = import_at(names, at);
        if (unique == 0u64 || !equal(name, previous)) {
            unsafe_position<NameBytes>(row_at<NameBytes>(names, unique)) = name;
            previous = name;
            unique += 1u64;
        }
        at += 1u64;
    }
    return unique;
}

struct NamePair { NameBytes first; NameBytes second; };

i32 compare_pair(NamePair left, NamePair right) on cpu {
    i32 first = compare_name(left.first, right.first);
    return first != 0 ? first : compare_name(left.second, right.second);
}

extern "C" bool pair_before(address left, address right, address context) on cpu {
    return compare_pair(unsafe_position<NamePair>(left), unsafe_position<NamePair>(right)) < i32(0);
}

extern "C" u64 order_name_pairs(address pairs, u64 count) on cpu {
    sort_rows<NamePair>(pairs, count, address(), c_function(pair_before));
    u64 unique = 0u64;
    u64 index = 0u64;
    NamePair previous = NamePair(empty(), empty());
    while (index < count) {
        NamePair pair = unsafe_position<NamePair>(row_at<NamePair>(pairs, index));
        if (unique == 0u64 || compare_pair(previous, pair) != 0) {
            unsafe_position<NamePair>(row_at<NamePair>(pairs, unique)) = pair;
            previous = pair;
            unique += 1u64;
        }
        index += 1u64;
    }
    return unique;
}

extern "C" void pair_functions(address names, u64 declarations, u64 implementations, address output) on cpu {
    u64 count = declarations + implementations;
    sort_rows<FunctionName>(names, count, address(), c_function(name_before));
    u64 first = 0u64;
    while (first < count) {
        FunctionName initial = unsafe_position<FunctionName>(row_at<FunctionName>(names, first));
        u64 end = first;
        u64 earliest = count;
        u64 declaration = declarations;
        while (end < count) {
            FunctionName current = unsafe_position<FunctionName>(row_at<FunctionName>(names, end));
            if (!equal(initial.name, current.name)) { break; }
            if (current.ordinal < declarations) {
                if (current.ordinal < declaration) { declaration = current.ordinal; }
            } else if (current.ordinal < earliest) {
                earliest = current.ordinal;
            }
            end += 1u64;
        }
        u64 at = first;
        while (at < end) {
            u64 ordinal = unsafe_position<FunctionName>(row_at<FunctionName>(names, at)).ordinal;
            u64 result = declaration;
            if (ordinal < declarations) { result = earliest - declarations; }
            unsafe_position<u64>(row_at<u64>(output, ordinal)) = result;
            at += 1u64;
        }
        first = end;
    }
}

extern "C" ModuleNameCounts classify_module_names(address local,u64 local_count,
    address references,u64 reference_count) on cpu {
    u64 defined = order_imports(local,local_count);
    u64 used = order_imports(references,reference_count);
    u64 next = 0u64;
    u64 external = 0u64;
    u64 index = 0u64;
    while (index < used) {
        NameBytes name = import_at(references,index);
        while (next < defined && compare_name(import_at(local,next),name) < 0) {
            next += 1u64;
        }
        if (next == defined || !equal(import_at(local,next),name)) {
            unsafe_position<NameBytes>(row_at<NameBytes>(references,external)) = name;
            external += 1u64;
        }
        index += 1u64;
    }
    return ModuleNameCounts(defined,external);
}
