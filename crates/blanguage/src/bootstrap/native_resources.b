module bootstrap.native_resources;
import bootstrap.bindings;

ResourceModule module_at(ResourceInput input,u64 i) on cpu {return unsafe_position<ResourceModule>(unsafe_address(address_bits(input.modules)+i*MODULE_BYTES));}
void module_put(ResourceInput input,u64 i,ResourceModule row) on cpu {unsafe_position<ResourceModule>(unsafe_address(address_bits(input.modules)+i*MODULE_BYTES))=row;}
ResourceLocal local_at(ResourceInput input,u64 i) on cpu {return unsafe_position<ResourceLocal>(unsafe_address(address_bits(input.locals)+i*LOCAL_BYTES));}
void local_put(ResourceInput input,u64 i,ResourceLocal row) on cpu {unsafe_position<ResourceLocal>(unsafe_address(address_bits(input.locals)+i*LOCAL_BYTES))=row;}
ResourceRuntime runtime_at(ResourceInput input,u64 i) on cpu {return unsafe_position<ResourceRuntime>(unsafe_address(address_bits(input.runtime)+i*RUNTIME_BYTES));}
void runtime_put(ResourceInput input,u64 i,ResourceRuntime row) on cpu {unsafe_position<ResourceRuntime>(unsafe_address(address_bits(input.runtime)+i*RUNTIME_BYTES))=row;}
u64 order_at(ResourceInput input,u64 i) on cpu {return unsafe_position<u64>(unsafe_address(address_bits(input.order)+i*8u64));}
void order_put(ResourceInput input,u64 i,u64 value) on cpu {unsafe_position<u64>(unsafe_address(address_bits(input.order)+i*8u64))=value;}
u8 byte_at(address data,u64 i) on cpu {return unsafe_position<u8>(unsafe_address(address_bits(data)+i));}
void put(address data,u64 i,u8 value) on cpu {unsafe_position<u8>(unsafe_address(address_bits(data)+i))=value;}
i32 compare(BindingBytes a,BindingBytes b) on cpu {
    u64 i=0u64;while(i<a.length&&i<b.length){u8 x=byte_at(a.bytes,i);u8 y=byte_at(b.bytes,i);if(x<y){return -1;}if(x>y){return i32(1);}i+=1u64;}
    if(a.length<b.length){return -1;}if(a.length>b.length){return i32(1);}return i32(0);
}
bool less(ResourceInput input,u64 a,u64 b) on cpu {i32 result=compare(module_at(input,a).prefix,module_at(input,b).prefix);if(result!=0){return result<0;}return a<b;}
void sift(ResourceInput input,u64 start,u64 end) on cpu {
    u64 root=start;while(root<end/2u64){u64 child=root*2u64+1u64;
        if(child+1u64<end&&less(input,order_at(input,child),order_at(input,child+1u64))){child+=1u64;}
        u64 a=order_at(input,root);u64 b=order_at(input,child);if(!less(input,a,b)){return;}
        order_put(input,root,b);order_put(input,child,a);root=child;
    }
}
void sort(ResourceInput input) on cpu {
    u64 i=0u64;while(i<input.module_count){order_put(input,i,i);i+=1u64;}
    u64 root=input.module_count/2u64;while(root>0u64){root-=1u64;sift(input,root,input.module_count);}
    u64 end=input.module_count;while(end>1u64){end-=1u64;u64 a=order_at(input,0u64);u64 b=order_at(input,end);order_put(input,0u64,b);order_put(input,end,a);sift(input,0u64,end);}
}
u64 first(ResourceInput input,BindingBytes name) on cpu {
    u64 begin=0u64;u64 end=input.module_count;while(begin<end){u64 middle=begin+(end-begin)/2u64;
        if(compare(module_at(input,order_at(input,middle)).prefix,name)<0){begin=middle+1u64;}else{end=middle;}}
    return order_at(input,begin);
}
ResourcePlan invalid(u64 error,u64 at) on cpu {return ResourcePlan(0u64,0u64,0u64,0u64,error,at);}
extern "C" ResourcePlan prepare(ResourceInput input) on cpu {
    if(input.module_count==0u64){return invalid(0u64,0u64);}
    sort(input);BindingBytes abi=input.root_abi;if(input.has_root_abi==0u64){abi=module_at(input,0u64).abi;}
    u64 i=0u64;
    while(i<input.module_count){ResourceModule row=module_at(input,i);
        if(compare(row.abi,abi)!=0){return invalid(1u64,i);}if(first(input,row.prefix)!=i){return invalid(2u64,i);}i+=1u64;
    }
    u64 types=input.types;u64 programs=input.programs;u64 at=8u64+8u64*input.module_count;i=0u64;
    while(i<input.module_count){
        ResourceModule row=module_at(input,i);if(at+4u64>4294967295u64){return invalid(5u64,i);}
        row.type_value=types;row.program_value=programs;row.factory_offset=8u64+8u64*i;row.type_offset=at;row.program_offset=at+4u64;
        row.factory=BindingBytes(unsafe_address(0u64),row.prefix.length+10u64);row.type_name=BindingBytes(unsafe_address(0u64),row.prefix.length+10u64);row.program_name=BindingBytes(unsafe_address(0u64),row.prefix.length+13u64);
        types+=row.types;if(types>4294967295u64){return invalid(3u64,i);}programs+=row.programs;if(programs>4294967295u64){return invalid(4u64,i);}
        module_put(input,i,row);at+=8u64;i+=1u64;
    }
    i=0u64;while(i<input.local_count){ResourceLocal row=local_at(input,i);if(at>4294967295u64){return invalid(5u64,input.module_count+i);}row.value=at;local_put(input,i,row);at+=8u64;i+=1u64;}
    u64 text=0u64;i=0u64;while(i<input.runtime_count){ResourceRuntime row=runtime_at(input,i);if(text>4294967295u64){return invalid(5u64,input.module_count+input.local_count+i);}row.value=text;row.relocation=text+2u64;runtime_put(input,i,row);text+=6u64;i+=1u64;}
    return ResourcePlan(at,text,1077936192u64,1615855648u64,0u64,0u64);
}
void literal(address output,u64 at,u64 value,u64 length) on cpu {u64 i=0u64;while(i<length){put(output,at+i,u8(value>>(i*8u64)));i+=1u64;}}
void copy(address output,BindingBytes source) on cpu {u64 i=0u64;while(i<source.length){put(output,i,byte_at(source.bytes,i));i+=1u64;}}
extern "C" void encode(ResourceInput input,ResourceOutput output,ResourcePlan plan) on cpu {
    u64 i=0u64;while(i<plan.data){put(output.data,i,u8(0));i+=1u64;}i=0u64;while(i<plan.text){put(output.text,i,u8(0));i+=1u64;}
    literal(output.data,0u64,input.module_count,8u64);i=0u64;
    while(i<input.module_count){
        ResourceModule row=module_at(input,i);literal(output.data,row.type_offset,row.type_value,4u64);literal(output.data,row.program_offset,row.program_value,4u64);
        copy(row.factory.bytes,row.prefix);literal(row.factory.bytes,row.prefix.length,7165919078637728351u64,8u64);literal(row.factory.bytes,row.prefix.length+8u64,29541u64,2u64);
        copy(row.type_name.bytes,row.prefix);literal(row.type_name.bytes,row.prefix.length,7017276058679997535u64,8u64);literal(row.type_name.bytes,row.prefix.length+8u64,25971u64,2u64);
        copy(row.program_name.bytes,row.prefix);literal(row.program_name.bytes,row.prefix.length,7881706611452047455u64,8u64);literal(row.program_name.bytes,row.prefix.length+8u64,435727458911u64,5u64);
        i+=1u64;
    }
    i=0u64;while(i<input.runtime_count){ResourceRuntime row=runtime_at(input,i);put(output.text,row.value,u8(255));put(output.text,row.value+1u64,u8(37));i+=1u64;}
}
