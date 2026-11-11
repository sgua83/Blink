module bootstrap.names;
NameBytes empty() on cpu {return NameBytes(unsafe_address(0u64),0u64);}
u8 byte_at(NameBytes value,u64 at) on cpu {return unsafe_position<u8>(unsafe_address(address_bits(value.bytes)+at));}
NameBytes import_at(address imports,u64 at) on cpu {return unsafe_position<NameBytes>(unsafe_address(address_bits(imports)+at*16u64));}
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

bool before(NameBytes left, NameBytes right) on cpu {
    u64 at = 0u64;
    while (at < left.length && at < right.length) {
        u8 a = byte_at(left, at);
        u8 b = byte_at(right, at);
        if (a != b) { return a < b; }
        at += 1u64;
    }
    return left.length < right.length;
}

void put_import(address names, u64 index, NameBytes name) on cpu {
    unsafe_position<NameBytes>(unsafe_address(address_bits(names) + index * size_of<NameBytes>())) = name;
}

void sift_import(address names, u64 start, u64 count) on cpu {
    NameBytes name = import_at(names, start);
    u64 root = start;
    while (root < count / 2u64) {
        u64 child = 2u64 * root + 1u64;
        if (child + 1u64 < count && before(import_at(names, child), import_at(names, child + 1u64))) {
            child += 1u64;
        }
        NameBytes next = import_at(names, child);
        if (!before(name, next)) { break; }
        put_import(names, root, next);
        root = child;
    }
    put_import(names, root, name);
}

extern "C" u64 order_imports(address names, u64 count) on cpu {
    u64 root = count / 2u64;
    while (root != 0u64) {
        root -= 1u64;
        sift_import(names, root, count);
    }
    u64 end = count;
    while (end > 1u64) {
        end -= 1u64;
        NameBytes first = import_at(names, 0u64);
        put_import(names, 0u64, import_at(names, end));
        put_import(names, end, first);
        sift_import(names, 0u64, end);
    }

    u64 unique = 0u64;
    u64 at = 0u64;
    NameBytes previous = empty();
    while (at < count) {
        NameBytes name = import_at(names, at);
        if (unique == 0u64 || !equal(name, previous)) {
            put_import(names, unique, name);
            previous = name;
            unique += 1u64;
        }
        at += 1u64;
    }
    return unique;
}
