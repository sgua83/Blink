module bootstrap.names;
extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap,u32 flags,usize bytes) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap,u32 flags,address rows,usize bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap,u32 flags,address rows) on cpu from "KERNEL32.dll";

extern "C" DependencyEntry enter_dependency(address path, NameBytes key) on cpu {
    DependencyPath active=unsafe_position<DependencyPath>(path);
    u64 at=active.count;
    while (at!=0u64) {
        at-=1u64;
        if (equal(import_at(active.rows,at),key)) {
            return DependencyEntry(DependencyStatus::Recursive,0u64);
        }
    }

    if (active.count==active.capacity) {
        u64 max_rows=9223372036854775807u64/size_of<NameBytes>();
        if (active.capacity==max_rows) {
            return DependencyEntry(DependencyStatus::Overflow,0u64);
        }
        u64 capacity=4u64;
        if (active.capacity!=0u64) {
            capacity=active.capacity<=max_rows/2u64 ? active.capacity*2u64 : max_rows;
        }
        u64 bytes=capacity*size_of<NameBytes>();
        handle heap=GetProcessHeap();
        address rows;
        if (active.capacity==0u64) { rows=HeapAlloc(heap,0,bytes); }
        else { rows=HeapReAlloc(heap,0,active.rows,bytes); }
        if (address_bits(rows)==0u64) {
            return DependencyEntry(DependencyStatus::Allocation,bytes);
        }
        active.rows=rows;
        active.capacity=capacity;
    }

    unsafe_position<NameBytes>(row_at<NameBytes>(active.rows,active.count))=key;
    active.count+=1u64;
    unsafe_position<DependencyPath>(path)=active;
    return DependencyEntry(DependencyStatus::Entered,0u64);
}

extern "C" void leave_dependency(address path) on cpu {
    unsafe_position<DependencyPath>(path).count-=1u64;
}

extern "C" void release_dependencies(address path) on cpu {
    address rows=unsafe_position<DependencyPath>(path).rows;
    if (address_bits(rows)!=0u64) { HeapFree(GetProcessHeap(),0,rows); }
}

NameBytes empty() on cpu {return NameBytes(unsafe_address(0u64),0u64);}
u8 byte_at(NameBytes value,u64 at) on cpu {return unsafe_position<u8>(unsafe_address(address_bits(value.bytes)+at));}
NameBytes import_at(address imports,u64 at) on cpu {return unsafe_position<NameBytes>(row_at<NameBytes>(imports,at));}
bool equal(NameBytes a,NameBytes b) on cpu {
    if(a.length!=b.length){return false;}
    u64 at=0u64;while(at<a.length){if(byte_at(a,at)!=byte_at(b,at)){return false;}at+=1u64;}return true;
}
u64 qualification_separator(NameBytes name) on cpu {
    u64 at=1u64;
    while (at<name.length) {
        if (byte_at(name,at-1u64)==58 && byte_at(name,at)==58) { return at-1u64; }
        at+=1u64;
    }
    return name.length;
}
bool directly_imported(NameBytes owner, u64 count, address context,
    cfn<NameBytes,address,u64> import_name) on cpu requires nonzero(import_name) {
    u64 index=0u64;
    while(index<count){
        if(equal(import_name(context,index),owner)){return true;}
        index+=1u64;
    }
    return false;
}
extern "C" NameResult resolve(NameBytes name, NameBytes module, u64 count, address context,
    cfn<NameBytes,address,u64> import_name, cfn<NameHit,address,NameBytes,NameBytes> lookup) on cpu
    requires nonzero(import_name) requires nonzero(lookup) {
    u64 separator=qualification_separator(name);
    if(separator<name.length){
        NameHit hit=lookup(context,empty(),name);
        NameBytes owner=NameBytes(name.bytes,separator);
        if(hit.present!=0u64&&(equal(owner,module)||(hit.public!=0u64&&directly_imported(owner,count,context,import_name)))){
            return NameResult(hit.name,1u64);
        }
    }
    NameHit local=lookup(context,module,name);
    if(local.present!=0u64){return NameResult(local.name,1u64);}

    NameBytes first=empty();
    u64 found=0u64;
    u64 index=0u64;
    while(index<count){
        NameHit hit=lookup(context,import_name(context,index),name);
        if(hit.public!=0u64){
            if(found==0u64){
                first=hit.name;
                found=1u64;
            } else if(!equal(first,hit.name)){
                return NameResult(empty(),2u64);
            }
        }
        index+=1u64;
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

bool local_module_name(address local, u64 count, NameBytes name) on cpu {
    u64 first = 0u64;
    u64 last = count;
    while (first < last) {
        u64 middle = first + (last - first) / 2u64;
        i32 order = compare_name(import_at(local, middle), name);
        if (order == 0) { return true; }
        if (order < 0) { first = middle + 1u64; }
        else { last = middle; }
    }
    return false;
}

extern "C" void collect_external_module_names(address local, u64 local_count,
    address references, cfn<NameBytes,address> next, address external,
    cfn<void,address,NameBytes> push) on cpu requires nonzero(next) requires nonzero(push) {
    NameBytes name = next(references);
    while (address_bits(name.bytes) != 0u64) {
        if (!local_module_name(local, local_count, name)) { push(external, name); }
        name = next(references);
    }
}
