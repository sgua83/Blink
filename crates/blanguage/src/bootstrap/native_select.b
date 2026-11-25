module bootstrap.native_select;
import bootstrap.bindings;

struct SelectState {SelectView view;BindingForest forest;u64 head;u64 tail;u64 local_count;u64 entry;bool runtime_import;};
struct SelectTarget {u64 object;u64 section;SelectResult result;};
BindingBytes empty() on cpu {return BindingBytes(unsafe_address(0u64),0u64);}
SelectResult result(u64 error,BindingBytes name,BindingBytes owner,u64 entry) on cpu {return SelectResult(error,name,owner,entry,EntryPlan(EntryProduct::None,0,0u64));}
SelectResult success(u64 entry) on cpu {return result(0u64,empty(),empty(),entry);}
SelectObject object_at(SelectView view,u64 i) on cpu {return unsafe_position<SelectObject>(unsafe_address(address_bits(view.objects)+i*OBJECT_BYTES));}
void object_put(SelectView view,u64 i,SelectObject row) on cpu {unsafe_position<SelectObject>(unsafe_address(address_bits(view.objects)+i*OBJECT_BYTES))=row;}
address section_address(SelectView view,u64 index) on cpu {
    return unsafe_address(address_bits(view.sections)+index*SECTION_BYTES);
}
SelectSection section_at(SelectView view,u64 i) on cpu {return unsafe_position<SelectSection>(section_address(view,i));}
SelectRelocation relocation_at(SelectView view,u64 i) on cpu {return unsafe_position<SelectRelocation>(unsafe_address(address_bits(view.relocations)+i*RELOCATION_BYTES));}
void relocation_put(SelectView view,u64 i,SelectRelocation row) on cpu {unsafe_position<SelectRelocation>(unsafe_address(address_bits(view.relocations)+i*RELOCATION_BYTES))=row;}
address symbol_address(SelectView view,u64 index) on cpu {
    return unsafe_address(address_bits(view.symbols)+index*SYMBOL_BYTES);
}
SelectSymbol symbol_at(SelectView view,u64 i) on cpu {return unsafe_position<SelectSymbol>(symbol_address(view,i));}
SelectImport import_row(SelectView view,u64 i) on cpu {return unsafe_position<SelectImport>(unsafe_address(address_bits(view.imports)+i*IMPORT_BYTES));}
address export_address(SelectView view,u64 index) on cpu {
    return unsafe_address(address_bits(view.exports)+index*EXPORT_BYTES);
}
address frame_address(SelectView view,u64 index) on cpu {
    return unsafe_address(address_bits(view.frames)+index*FRAME_BYTES);
}
SelectLocal local_at(SelectView view,u64 i) on cpu {return unsafe_position<SelectLocal>(unsafe_address(address_bits(view.locals)+i*LOCAL_BYTES));}
void local_put(SelectView view,u64 i,SelectLocal row) on cpu {unsafe_position<SelectLocal>(unsafe_address(address_bits(view.locals)+i*LOCAL_BYTES))=row;}
u64 index_at(address data,u64 i) on cpu {return unsafe_position<u64>(unsafe_address(address_bits(data)+i*8u64));}
void index_put(address data,u64 i,u64 value) on cpu {unsafe_position<u64>(unsafe_address(address_bits(data)+i*8u64))=value;}
BindingBytes name_at(address data,u64 i) on cpu {return unsafe_position<BindingBytes>(unsafe_address(address_bits(data)+i*16u64));}
address root_address(address roots, u64 index) on cpu {
    return unsafe_address(address_bits(roots) + index * size_of<SelectRoot>());
}
SelectRoot root_at(SelectParams params,u64 i) on cpu {return unsafe_position<SelectRoot>(root_address(params.roots,i));}

extern "C" u64 bind_root_contracts(address roots,u64 count,address context,u64 sources,
    cfn<BindingBytes,address,u64,BindingBytes> lookup) on cpu requires nonzero(lookup) {
    u64 index=0u64;
    while(index<count) {
        address slot=root_address(roots,index);
        SelectRoot root=unsafe_position<SelectRoot>(slot);
        BindingBytes contract=empty();
        u64 source=sources;
        while(source!=0u64) {
            source-=1u64;
            contract=lookup(context,source,root.name);
            if(address_bits(contract.bytes)!=0u64){break;}
        }
        if(address_bits(contract.bytes)==0u64){return index+1u64;}
        root.contract=contract;
        unsafe_position<SelectRoot>(slot)=root;
        index+=1u64;
    }
    return 0u64;
}
u8 byte_at(BindingBytes name,u64 i) on cpu {return unsafe_position<u8>(unsafe_address(address_bits(name.bytes)+i));}
bool equal(BindingBytes a,BindingBytes b) on cpu {return bytes_compare(a,b)==0;}
u64 ordered(u64 i,u64 entry) on cpu {if(i==0u64){return entry;}if(i==entry){return 0u64;}return i;}
bool suffixed(BindingBytes name,BindingBytes prefix,u64 low,u64 high,u64 length) on cpu {
    if(name.length!=prefix.length+length){return false;}
    u64 i=0u64;while(i<prefix.length){if(byte_at(name,i)!=byte_at(prefix,i)){return false;}i+=1u64;}
    i=0u64;while(i<length){u64 value=low;u64 shift=i*8u64;if(i>=8u64){value=high;shift=(i-8u64)*8u64;}
        if(byte_at(name,prefix.length+i)!=u8(value>>shift)){return false;}i+=1u64;}return true;
}
bool select_runtime(SelectView view,BindingBytes name,bool collect) on cpu {
    u64 i=0u64;
    while(i<view.runtime_count){
        address at=unsafe_address(address_bits(view.runtime)+i*size_of<SelectRuntime>());
        SelectRuntime symbol=unsafe_position<SelectRuntime>(at);
        if(equal(name,symbol.name)){
            if(collect&&symbol.used==0u64){
                symbol.used=1u64;
                unsafe_position<SelectRuntime>(at)=symbol;
            }
            return true;
        }
        i+=1u64;
    }
    return false;
}
bool match_export(SelectView view,SelectObject object,BindingBytes name,bool retain) on cpu {
    u64 index=object.export_start;
    while(index<object.export_start+object.export_count){
        address at=export_address(view,index);
        SelectExport row=unsafe_position<SelectExport>(at);
        if(equal(row.name,name)){
            if(retain&&row.keep==0u64){unsafe_position<SelectExport>(at).keep=1u64;}
            return true;
        }
        index+=1u64;
    }
    return false;
}
SelectResult define(inout SelectState state,u64 object_index) on cpu {
    SelectObject object=object_at(state.view,object_index);u64 i=object.symbol_start;
    while(i<object.symbol_start+object.symbol_count){SelectSymbol symbol=symbol_at(state.view,i);BindingKey key=BindingKey(symbol.name,empty());
        BindingInsert insertion=insert_binding(state.forest,state.forest.names,key,i+1u64,false);
        if(!insertion.inserted){return result(2u64,symbol.name,empty(),state.entry);}
        state.forest.names=insertion.root;i+=1u64;}
    return success(state.entry);
}
void enqueue(inout SelectState state,u64 section_index) on cpu {
    address at=section_address(state.view,section_index);
    if(unsafe_position<SelectSection>(at).live==0u64){
        unsafe_position<SelectSection>(at).live=1u64;
        index_put(state.view.pending,state.tail,section_index);
        state.tail+=1u64;
    }
}
void include(inout SelectState state,u64 object_index,u64 section) on cpu {
    SelectObject object=object_at(state.view,object_index);
    if(object.selected==0u64){object.selected=1u64;object_put(state.view,object_index,object);u64 i=object.section_start;
        while(i<object.section_start+object.section_count){if((section_at(state.view,i).flags&4096u64)==0u64){enqueue(state,i);}i+=1u64;}}
    enqueue(state,object.section_start+section);
}
SelectTarget resolve(inout SelectState state,u64 definition,BindingBytes name,BindingBytes contract,address context,cfn<SelectAppend,address,address,u64,BindingBytes> extract) on cpu requires nonzero(extract) {
    u64 at=definition;
    if(at==0u64){
        u64 library=0u64;u64 object_index=0u64;bool found=false;
        while(library<state.view.library_count){
            SelectAppend appended=extract(context,address_of(state.view),library,name);
            if(appended==SelectAppend::Failed){return SelectTarget(0u64,0u64,result(7u64,name,empty(),state.entry));}
            if(appended==SelectAppend::Appended){
                state.forest.nodes=state.view.nodes;
                object_index=state.view.object_count-1u64;
                found=true;
                break;
            }
            library+=1u64;
        }
        if(!found){return SelectTarget(0u64,0u64,result(3u64,name,empty(),state.entry));}
        SelectResult defined=define(state,object_index);if(defined.error!=0u64){return SelectTarget(0u64,0u64,defined);}
        at=find(state.forest,state.forest.names,BindingKey(name,empty()));
        if(at==0u64){return SelectTarget(0u64,0u64,result(5u64,name,object_at(state.view,object_index).name,state.entry));}
    }
    SelectSymbol symbol=symbol_at(state.view,node_at(state.forest,at).value-1u64);
    if(!equal(symbol.contract,contract)){return SelectTarget(0u64,0u64,result(4u64,name,empty(),state.entry));}
    return SelectTarget(symbol.owner,symbol.section,success(state.entry));
}
bool defer_unbound(SelectState state,BindingBytes name,bool runtime) on cpu {
    if(find(state.forest,state.forest.imports,BindingKey(name,empty()))!=0u64||runtime){return true;}
    u64 i=0u64;while(i<state.view.object_count){SelectObject object=object_at(state.view,i);
        if(object.has_resources!=0u64&&(suffixed(name,object.prefix,7017276058679997535u64,25971u64,10u64)||suffixed(name,object.prefix,7881706611452047455u64,435727458911u64,13u64))){return true;}i+=1u64;}return false;
}
SelectImport required_import(SelectView view,SelectObject object,BindingBytes name) on cpu {
    u64 begin=object.import_start;u64 end=begin+object.import_count;while(begin<end){u64 middle=begin+(end-begin)/2u64;
        SelectImport row=import_row(view,middle);i32 order=bytes_compare(row.name,name);if(order==0){return row;}if(order<0){begin=middle+1u64;}else{end=middle;}}
    return SelectImport(empty(),empty());
}
EntryPlan finish(SelectView view,bool native_mode,u64 entry,bool runtime_import) on cpu {
    bool retained_archive=false;
    bool uses_runtime=runtime_import;
    u64 stack_bytes=0u64;
    u64 o=0u64;while(o<view.object_count){SelectObject object=object_at(view,o);
        if(object.selected!=0u64){
            u64 mapping=0u64;u64 i=object.section_start;
            while(i<object.section_start+object.section_count){
                address at=section_address(view,i);
                if(unsafe_position<SelectSection>(at).live!=0u64){
                    unsafe_position<SelectSection>(at).mapping=mapping;
                    mapping+=1u64;
                }
                i+=1u64;
            }
            object.keep_archive=object.archive;if(native_mode||object.borrowed!=0u64){object.keep_archive=0u64;}
            if(object.kind==0u64&&object.keep_archive!=0u64){
                bool used=false;
                i=object.export_start;
                while(i<object.export_start+object.export_count){
                    address at=export_address(view,i);
                    SelectExport row=unsafe_position<SelectExport>(at);
                    if(row.keep!=0u64){used=true;}
                    else if(object.has_resources!=0u64&&suffixed(row.name,object.prefix,7165919078637728351u64,29541u64,10u64)){
                        unsafe_position<SelectExport>(at).keep=1u64;
                    }
                    i+=1u64;
                }
                if(!used){object.keep_archive=0u64;}
            }
            retained_archive=retained_archive||object.keep_archive!=0u64;
            uses_runtime=uses_runtime||(object.keep_archive!=0u64&&object.has_resources!=0u64);
            if(object.kind!=2u64&&object.stack_bytes>stack_bytes){stack_bytes=object.stack_bytes;}
            i=object.section_start;while(i<object.section_start+object.section_count){SelectSection section=section_at(view,i);if(section.live!=0u64){
                u64 r=section.relocation_start;while(r<section.relocation_start+section.relocation_count){SelectRelocation row=relocation_at(view,r);
                    if(row.kind==0u64){row.section=section_at(view,object.section_start+row.section).mapping;relocation_put(view,r,row);}r+=1u64;}}
                i+=1u64;
            }
            i=object.symbol_start;
            while(i<object.symbol_start+object.symbol_count){
                address at=symbol_address(view,i);
                u64 input_section=unsafe_position<SelectSymbol>(at).section;
                SelectSection section=section_at(view,object.section_start+input_section);
                if(section.live!=0u64){
                    unsafe_position<SelectSymbol>(at).section=section.mapping;
                    unsafe_position<SelectSymbol>(at).keep=1u64;
                }
                i+=1u64;
            }
            if(object.borrowed==0u64){
                i=object.frame_start;
                while(i<object.frame_start+object.frame_count){
                    address at=frame_address(view,i);
                    SelectFrame frame=unsafe_position<SelectFrame>(at);
                    if(frame.present==0u64){
                        if(object.keep_archive!=0u64){unsafe_position<SelectFrame>(at).keep=1u64;}
                    }else{
                        SelectSection section=section_at(view,object.section_start+frame.section);
                        if(section.live!=0u64){
                            unsafe_position<SelectFrame>(at).mapping=section.mapping;
                            unsafe_position<SelectFrame>(at).keep=1u64;
                        }
                    }
                    i+=1u64;
                }
            }
            object_put(view,o,object);
        }o+=1u64;
    }
    if(native_mode){return EntryPlan(EntryProduct::None,0,0u64);}
    if(object_at(view,entry).kind==2u64){
        if(retained_archive){return EntryPlan(EntryProduct::Dll,1,0u64);}
        return EntryPlan(EntryProduct::DllWithoutArchive,1,0u64);
    }
    u64 stack=u64(u32(stack_bytes))+1048576u64;
    u64 reserve=((stack+65535u64)/65536u64)*65536u64;
    u32 runtime=uses_runtime ? 1 : 0;
    if(retained_archive){return EntryPlan(EntryProduct::HostApplication,runtime,reserve);}
    return EntryPlan(EntryProduct::Pe,runtime,reserve);
}
extern "C" SelectResult choose(SelectView input,SelectParams params,address context,
    cfn<SelectAppend,address,address,u64,BindingBytes> extract,cfn<u64,address,u64> native_archive,
    cfn<SelectAppend,address,address,SelectResource> resources,cfn<BindingBytes,address,address,BindingBytes> slot) on cpu
    requires nonzero(extract) requires nonzero(native_archive) requires nonzero(resources) requires nonzero(slot) {
    SelectState state=SelectState(input,BindingForest(input.nodes,0u64,0u64,0u64),0u64,0u64,0u64,0u64,false);
    if(params.native_mode==0u64){u64 count=0u64;u64 i=0u64;while(i<input.object_count){if(object_at(input,i).kind!=0u64){state.entry=i;count+=1u64;}i+=1u64;}
        if(count!=1u64){return result(1u64,empty(),empty(),0u64);}}
    u64 o=0u64;while(o<state.view.object_count){SelectResult defined=define(state,ordered(o,state.entry));if(defined.error!=0u64){return defined;}o+=1u64;}
    if(params.native_mode==0u64){SelectObject entry=object_at(state.view,state.entry);
        if(entry.kind==2u64){entry.selected=1u64;object_put(state.view,state.entry,entry);u64 i=entry.section_start;while(i<entry.section_start+entry.section_count){enqueue(state,i);i+=1u64;}}
        else{include(state,state.entry,0u64);}
    }else{
        u64 i=0u64;
        while(i<params.root_count){
            SelectRoot root=root_at(params,i);
            u64 definition=find(state.forest,state.forest.names,BindingKey(root.name,empty()));
            SelectTarget target=resolve(state,definition,root.name,root.contract,context,extract);
            if(target.result.error!=0u64){return target.result;}
            include(state,target.object,target.section);
            i+=1u64;
        }
    }
    bool bind=params.native_mode!=0u64;
    while(true){
        while(state.head<state.tail){u64 section_index=index_at(state.view.pending,state.head);state.head+=1u64;SelectSection section=section_at(state.view,section_index);u64 object_index=section.owner;SelectObject object=object_at(state.view,object_index);
            u64 a=0u64;
            while(a<section.associated_count){
                include(state,object_index,index_at(section.associated,a));
                a+=1u64;
            }
            bool retain_exports=params.native_mode==0u64&&object.kind==0u64&&object.borrowed==0u64;
            u64 r=section.relocation_start;
            while(r<section.relocation_start+section.relocation_count){
                SelectRelocation relocation=relocation_at(state.view,r);
                bool runtime_import=relocation.kind==2u64&&equal(relocation.dll,params.runtime_library);
                state.runtime_import=state.runtime_import||runtime_import;
                if(runtime_import&&object.archive!=0u64&&match_export(state.view,object,relocation.name,retain_exports)){
                    u64 c=object.code_start;while(c<object.code_start+object.code_count){BindingBytes name=name_at(state.view.code,c);u64 at=find(state.forest,state.forest.names,BindingKey(name,empty()));
                        if(at==0u64){return result(6u64,name,empty(),state.entry);}SelectSymbol symbol=symbol_at(state.view,node_at(state.forest,at).value-1u64);
                        if(symbol.owner!=object_index){return result(6u64,name,empty(),state.entry);}include(state,object_index,symbol.section);c+=1u64;}
                    if(params.native_mode!=0u64){
                        if(object.active_archive==0u64){
                            u64 library_count=native_archive(context,object_index);
                            if(library_count==0u64){return result(7u64,empty(),empty(),state.entry);}
                            state.view.library_count=library_count;
                            object.active_archive=1u64;
                            object_put(state.view,object_index,object);
                        }
                        u64 definition=find(state.forest,state.forest.names,BindingKey(relocation.name,empty()));
                        SelectTarget target=resolve(state,definition,relocation.name,empty(),context,extract);if(target.result.error!=0u64){return target.result;}include(state,target.object,target.section);
                        // All local slots use the same runtime DLL prefix, so name and slot order agree.
                        u64 local=state.forest.imports;u64 found=0u64;
                        while(local!=0u64){BindingNode node=node_at(state.forest,local);SelectLocal row=local_at(state.view,node.value-1u64);i32 order=bytes_compare(relocation.name,row.name);if(order==0){found=local;break;}if(order<0){local=node.left;}else{local=node.right;}}
                        u64 index=0u64;
                        if(found==0u64){
                            BindingBytes name=slot(context,address_of(state.view),relocation.name);
                            state.forest.nodes=state.view.nodes;
                            index=state.local_count;
                            state.local_count+=1u64;
                            local_put(state.view,index,SelectLocal(name,relocation.name));
                            BindingInsert item=insert_binding(state.forest,state.forest.imports,BindingKey(name,empty()),index+1u64,false);
                            state.forest.imports=item.root;
                        }else{index=node_at(state.forest,found).value-1u64;}
                        relocation.rewrite=index+1u64;relocation_put(state.view,r,relocation);
                    }
                }
                if(relocation.kind==0u64){include(state,object_index,relocation.section);}
                else if(relocation.kind==1u64){
                    bool runtime=params.native_mode!=0u64&&select_runtime(state.view,relocation.name,bind);
                    u64 definition=find(state.forest,state.forest.names,BindingKey(relocation.name,empty()));
                    if(definition!=0u64||params.native_mode==0u64||!defer_unbound(state,relocation.name,runtime)){
                        SelectImport imported=required_import(state.view,object,relocation.name);
                        if(imported.name.length==0u64){return result(8u64,relocation.name,object.name,state.entry);}
                        SelectTarget target=resolve(state,definition,relocation.name,imported.contract,context,extract);
                        if(target.result.error!=0u64){return target.result;}
                        include(state,target.object,target.section);
                    }
                }
                r+=1u64;
            }
        }
        if(!bind){break;}bind=false;
        SelectResource request=SelectResource(state.view.runtime,state.view.runtime_count,state.view.locals,state.local_count,params.types,params.programs);
        SelectAppend appended=resources(context,address_of(state.view),request);
        if(appended==SelectAppend::Failed){return result(7u64,empty(),empty(),state.entry);}
        if(appended==SelectAppend::Appended){
            state.forest.nodes=state.view.nodes;
            u64 index=state.view.object_count-1u64;
            SelectResult defined=define(state,index);
            if(defined.error!=0u64){return defined;}
            SelectObject object=object_at(state.view,index);
            object.selected=1u64;
            object_put(state.view,index,object);
            u64 s=object.section_start;
            while(s<object.section_start+object.section_count){enqueue(state,s);s+=1u64;}
        }
    }
    EntryPlan plan=finish(state.view,params.native_mode!=0u64,state.entry,state.runtime_import);
    return SelectResult(0u64,empty(),empty(),state.entry,plan);
}
