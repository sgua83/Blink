module bootstrap.bindings;

BindingBytes empty() on cpu {return BindingBytes(unsafe_address(0u64),0u64);}
BindingNode node_at(BindingForest forest,u64 index) on cpu {
    return unsafe_position<BindingNode>(unsafe_address(address_bits(forest.nodes)+(index-1u64)*NODE_BYTES));
}
void node_put(BindingForest forest,u64 index,BindingNode value) on cpu {
    unsafe_position<BindingNode>(unsafe_address(address_bits(forest.nodes)+(index-1u64)*NODE_BYTES))=value;
}
i32 bytes_compare(BindingBytes left,BindingBytes right) on cpu {
    u64 count=left.length;if(right.length<count){count=right.length;}
    u64 i=0u64;
    while(i<count){
        u32 a=u32(unsafe_position<u8>(unsafe_address(address_bits(left.bytes)+i)));
        u32 b=u32(unsafe_position<u8>(unsafe_address(address_bits(right.bytes)+i)));
        if(a<b){return -1;}if(a>b){return i32(1);}i+=1u64;
    }
    if(left.length<right.length){return -1;}if(left.length>right.length){return i32(1);}return i32(0);
}
i32 key_compare(BindingKey left,BindingKey right) on cpu {
    i32 first=bytes_compare(left.first,right.first);if(first!=0){return first;}return bytes_compare(left.second,right.second);
}
u64 find(BindingForest forest,u64 root,BindingKey key) on cpu {
    u64 at=root;
    while(at!=0u64){
        BindingNode node=node_at(forest,at);i32 order=key_compare(key,node.key);
        if(order==0){return at;}if(order<0){at=node.left;}else{at=node.right;}
    }
    return 0u64;
}
u64 color(BindingForest forest,u64 at) on cpu {
    if(at==0u64){return 0u64;}return node_at(forest,at).red;
}
void paint(BindingForest forest,u64 at,u64 red) on cpu {
    BindingNode node=node_at(forest,at);node.red=red;node_put(forest,at,node);
}
u64 rotate_left(BindingForest forest,u64 root,u64 at) on cpu {
    BindingNode left=node_at(forest,at);u64 right_id=left.right;BindingNode right=node_at(forest,right_id);
    left.right=right.left;
    if(right.left!=0u64){
        BindingNode child=node_at(forest,right.left);child.parent=at;node_put(forest,right.left,child);
    }
    right.parent=left.parent;u64 result=root;
    if(left.parent==0u64){result=right_id;}
    else{
        BindingNode parent=node_at(forest,left.parent);
        if(parent.left==at){parent.left=right_id;}else{parent.right=right_id;}node_put(forest,left.parent,parent);
    }
    right.left=at;left.parent=right_id;node_put(forest,at,left);node_put(forest,right_id,right);return result;
}
u64 rotate_right(BindingForest forest,u64 root,u64 at) on cpu {
    BindingNode right=node_at(forest,at);u64 left_id=right.left;BindingNode left=node_at(forest,left_id);
    right.left=left.right;
    if(left.right!=0u64){
        BindingNode child=node_at(forest,left.right);child.parent=at;node_put(forest,left.right,child);
    }
    left.parent=right.parent;u64 result=root;
    if(right.parent==0u64){result=left_id;}
    else{
        BindingNode parent=node_at(forest,right.parent);
        if(parent.right==at){parent.right=left_id;}else{parent.left=left_id;}node_put(forest,right.parent,parent);
    }
    left.right=at;right.parent=left_id;node_put(forest,at,right);node_put(forest,left_id,left);return result;
}
u64 balance(BindingForest forest,u64 root,u64 inserted) on cpu {
    u64 result=root;u64 at=inserted;
    while(true){
        u64 parent_id=node_at(forest,at).parent;
        if(parent_id==0u64 || color(forest,parent_id)==0u64){break;}
        BindingNode parent=node_at(forest,parent_id);u64 grand_id=parent.parent;BindingNode grand=node_at(forest,grand_id);
        if(parent_id==grand.left){
            u64 uncle=grand.right;
            if(color(forest,uncle)!=0u64){
                paint(forest,parent_id,0u64);paint(forest,uncle,0u64);paint(forest,grand_id,1u64);at=grand_id;
            }else{
                if(at==parent.right){at=parent_id;result=rotate_left(forest,result,at);}
                parent_id=node_at(forest,at).parent;grand_id=node_at(forest,parent_id).parent;
                paint(forest,parent_id,0u64);paint(forest,grand_id,1u64);result=rotate_right(forest,result,grand_id);
            }
        }else{
            u64 uncle=grand.left;
            if(color(forest,uncle)!=0u64){
                paint(forest,parent_id,0u64);paint(forest,uncle,0u64);paint(forest,grand_id,1u64);at=grand_id;
            }else{
                if(at==parent.left){at=parent_id;result=rotate_right(forest,result,at);}
                parent_id=node_at(forest,at).parent;grand_id=node_at(forest,parent_id).parent;
                paint(forest,parent_id,0u64);paint(forest,grand_id,1u64);result=rotate_left(forest,result,grand_id);
            }
        }
    }
    paint(forest,result,0u64);return result;
}
BindingInsert insert_binding(inout BindingForest forest,u64 root,BindingKey key,u64 value,bool replace) on cpu {
    u64 parent=0u64;u64 at=root;i32 order=i32(0);
    while(at!=0u64){
        parent=at;BindingNode node=node_at(forest,at);order=key_compare(key,node.key);
        if(order==0){
            if(replace){node.value=value;node_put(forest,at,node);}
            return BindingInsert(root,false);
        }
        if(order<0){at=node.left;}else{at=node.right;}
    }
    forest.count+=1u64;u64 index=forest.count;
    node_put(forest,index,BindingNode(key,value,parent,0u64,0u64,1u64));
    u64 next_root=root;
    if(parent==0u64){next_root=index;}
    else{
        BindingNode node=node_at(forest,parent);if(order<0){node.left=index;}else{node.right=index;}node_put(forest,parent,node);
    }
    next_root=balance(forest,next_root,index);return BindingInsert(next_root,true);
}
u64 first(BindingForest forest,u64 root) on cpu {
    u64 at=root;
    while(at!=0u64){u64 left=node_at(forest,at).left;if(left==0u64){break;}at=left;}return at;
}
u64 next(BindingForest forest,u64 index) on cpu {
    BindingNode node=node_at(forest,index);if(node.right!=0u64){return first(forest,node.right);}
    u64 at=index;u64 parent=node.parent;
    while(parent!=0u64){BindingNode value=node_at(forest,parent);if(value.left==at){return parent;}at=parent;parent=value.parent;}
    return 0u64;
}

u64 digits(u64 value) on cpu {
    u64 remaining=value;u64 count=1u64;while(remaining>=10u64){remaining/=10u64;count+=1u64;}return count;
}
void literal(address output,u64 at,u64 value,u64 length) on cpu {
    u64 i=0u64;while(i<length){unsafe_position<u8>(unsafe_address(address_bits(output)+at+i))=u8(u32(value>>(i*8u64))&255);i+=1u64;}
}
void copy(address output,u64 at,BindingBytes value) on cpu {
    u64 i=0u64;while(i<value.length){unsafe_position<u8>(unsafe_address(address_bits(output)+at+i))=unsafe_position<u8>(unsafe_address(address_bits(value.bytes)+i));i+=1u64;}
}
void decimal(address output,u64 at,u64 value) on cpu {
    u64 remaining=value;u64 count=digits(value);
    while(count>0u64){count-=1u64;unsafe_position<u8>(unsafe_address(address_bits(output)+at+count))=u8(48+u32(remaining%10u64));remaining/=10u64;}
}
extern "C" u64 import_name_size(BindingBytes dll,BindingBytes name) on cpu {return 16u64+digits(dll.length)+dll.length+name.length;}
extern "C" void import_name(BindingBytes dll,BindingBytes name,address output) on cpu {
    // "__blink_import$<dll byte length>$<dll><name>"
    literal(output,0u64,6875710655180201823u64,8u64);literal(output,8u64,10261134006316393u64,7u64);
    decimal(output,15u64,dll.length);u64 begin=15u64+digits(dll.length);
    literal(output,begin,36u64,1u64);copy(output,begin+1u64,dll);copy(output,begin+1u64+dll.length,name);
}
extern "C" u64 host_anchor_name_size(HostAnchorName name) on cpu {
    return 23u64+name.identity.length+digits(name.section)+digits(u64(name.value));
}
extern "C" void write_host_anchor_name(HostAnchorName name,address output) on cpu {
    // "__blink_host_section$<identity>$<section>$<value>"
    literal(output,0u64,6875710655180201823u64,8u64);
    literal(output,8u64,7162257636215451496u64,8u64);
    literal(output,16u64,156471617908u64,5u64);
    copy(output,21u64,name.identity);
    u64 separator=21u64+name.identity.length;
    literal(output,separator,36u64,1u64);
    decimal(output,separator+1u64,name.section);
    separator+=1u64+digits(name.section);
    literal(output,separator,36u64,1u64);
    decimal(output,separator+1u64,u64(name.value));
}

u64 section_symbol_name_size(BindingBytes module_name,u64 section) on cpu {
    return 17u64+module_name.length+digits(section);
}
void section_symbol_name(BindingBytes module_name,u64 section,address output) on cpu {
    // "__blink_section$"
    literal(output,0u64,6875710655180201823u64,8u64);
    literal(output,8u64,2625158131518498163u64,8u64);
    copy(output,16u64,module_name);
    literal(output,16u64+module_name.length,36u64,1u64);
    decimal(output,17u64+module_name.length,section);
}

struct MsvcOrder { u64 count; u64 needs_sdk; };

address msvc_name_at(address rows, u64 index) on cpu {
    return unsafe_address(address_bits(rows) + index * size_of<MsvcName>());
}

MsvcOrder order_msvc_names(address rows, u64 count, BindingBytes sdk_prefix, address nodes) on cpu {
    BindingForest forest = BindingForest(nodes, 0u64, 0u64, 0u64);
    u64 root = 0u64;
    u64 index = 0u64;
    while (index < count) {
        BindingBytes name = unsafe_position<MsvcName>(msvc_name_at(rows, index)).name;
        root = insert_binding(forest, root, BindingKey(name, empty()), 0u64, false).root;
        index += 1u64;
    }
    u64 written = 0u64;
    bool needs_sdk = false;
    u64 at = first(forest, root);
    while (at != 0u64) {
        BindingBytes name = node_at(forest, at).key.first;
        bool sdk = sdk_prefix.length != 0u64 && name.length >= sdk_prefix.length &&
            bytes_compare(BindingBytes(name.bytes, sdk_prefix.length), sdk_prefix) == 0;
        unsafe_position<MsvcName>(msvc_name_at(rows, written)) = MsvcName(name, u64(sdk));
        needs_sdk = needs_sdk || sdk;
        written += 1u64;
        at = next(forest, at);
    }
    return MsvcOrder(written, u64(needs_sdk));
}

extern "C" MsvcNamePlan select_msvc_names(address libraries, u64 library_count,
    address exports, u64 export_count, address prefix, u64 prefix_length, address nodes) on cpu {
    let libs = order_msvc_names(libraries, library_count, BindingBytes(prefix, prefix_length), nodes);
    let syms = order_msvc_names(exports, export_count, empty(), nodes);
    return MsvcNamePlan(libs.count, syms.count, libs.needs_sdk);
}
