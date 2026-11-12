module bootstrap.native_select;
import bootstrap.bindings;

struct SelectState {SelectView view;BindingForest forest;u64 runtime_root;u64 head;u64 tail;u64 local_count;u64 entry;};
struct SelectTarget {u64 object;u64 section;SelectResult result;};
BindingBytes empty() on cpu {return BindingBytes(unsafe_address(0u64),0u64);}
SelectResult result(u64 error,BindingBytes name,BindingBytes owner,u64 entry) on cpu {return SelectResult(error,name,owner,entry);}
SelectResult success(u64 entry) on cpu {return result(0u64,empty(),empty(),entry);}
SelectObject object_at(SelectView view,u64 i) on cpu {return unsafe_position<SelectObject>(unsafe_address(address_bits(view.objects)+i*OBJECT_BYTES));}
void object_put(SelectView view,u64 i,SelectObject row) on cpu {unsafe_position<SelectObject>(unsafe_address(address_bits(view.objects)+i*OBJECT_BYTES))=row;}
SelectSection section_at(SelectView view,u64 i) on cpu {return unsafe_position<SelectSection>(unsafe_address(address_bits(view.sections)+i*SECTION_BYTES));}
void section_put(SelectView view,u64 i,SelectSection row) on cpu {unsafe_position<SelectSection>(unsafe_address(address_bits(view.sections)+i*SECTION_BYTES))=row;}
SelectRelocation relocation_at(SelectView view,u64 i) on cpu {return unsafe_position<SelectRelocation>(unsafe_address(address_bits(view.relocations)+i*RELOCATION_BYTES));}
void relocation_put(SelectView view,u64 i,SelectRelocation row) on cpu {unsafe_position<SelectRelocation>(unsafe_address(address_bits(view.relocations)+i*RELOCATION_BYTES))=row;}
SelectSymbol symbol_at(SelectView view,u64 i) on cpu {return unsafe_position<SelectSymbol>(unsafe_address(address_bits(view.symbols)+i*SYMBOL_BYTES));}
void symbol_put(SelectView view,u64 i,SelectSymbol row) on cpu {unsafe_position<SelectSymbol>(unsafe_address(address_bits(view.symbols)+i*SYMBOL_BYTES))=row;}
SelectImport import_row(SelectView view,u64 i) on cpu {return unsafe_position<SelectImport>(unsafe_address(address_bits(view.imports)+i*IMPORT_BYTES));}
SelectExport export_at(SelectView view,u64 i) on cpu {return unsafe_position<SelectExport>(unsafe_address(address_bits(view.exports)+i*EXPORT_BYTES));}
void export_put(SelectView view,u64 i,SelectExport row) on cpu {unsafe_position<SelectExport>(unsafe_address(address_bits(view.exports)+i*EXPORT_BYTES))=row;}
SelectFrame frame_at(SelectView view,u64 i) on cpu {return unsafe_position<SelectFrame>(unsafe_address(address_bits(view.frames)+i*FRAME_BYTES));}
void frame_put(SelectView view,u64 i,SelectFrame row) on cpu {unsafe_position<SelectFrame>(unsafe_address(address_bits(view.frames)+i*FRAME_BYTES))=row;}
SelectLocal local_at(SelectView view,u64 i) on cpu {return unsafe_position<SelectLocal>(unsafe_address(address_bits(view.locals)+i*LOCAL_BYTES));}
void local_put(SelectView view,u64 i,SelectLocal row) on cpu {unsafe_position<SelectLocal>(unsafe_address(address_bits(view.locals)+i*LOCAL_BYTES))=row;}
u64 index_at(address data,u64 i) on cpu {return unsafe_position<u64>(unsafe_address(address_bits(data)+i*8u64));}
void index_put(address data,u64 i,u64 value) on cpu {unsafe_position<u64>(unsafe_address(address_bits(data)+i*8u64))=value;}
BindingBytes name_at(address data,u64 i) on cpu {return unsafe_position<BindingBytes>(unsafe_address(address_bits(data)+i*16u64));}
void name_put(address data,u64 i,BindingBytes value) on cpu {unsafe_position<BindingBytes>(unsafe_address(address_bits(data)+i*16u64))=value;}
SelectRoot root_at(SelectParams params,u64 i) on cpu {return unsafe_position<SelectRoot>(unsafe_address(address_bits(params.roots)+i*32u64));}
u8 byte_at(BindingBytes name,u64 i) on cpu {return unsafe_position<u8>(unsafe_address(address_bits(name.bytes)+i));}
bool equal(BindingBytes a,BindingBytes b) on cpu {return bytes_compare(a,b)==0;}
u64 ordered(u64 i,u64 entry) on cpu {if(i==0u64){return entry;}if(i==entry){return 0u64;}return i;}
bool suffixed(BindingBytes name,BindingBytes prefix,u64 low,u64 high,u64 length) on cpu {
    if(name.length!=prefix.length+length){return false;}
    u64 i=0u64;while(i<prefix.length){if(byte_at(name,i)!=byte_at(prefix,i)){return false;}i+=1u64;}
    i=0u64;while(i<length){u64 value=low;u64 shift=i*8u64;if(i>=8u64){value=high;shift=(i-8u64)*8u64;}
        if(byte_at(name,prefix.length+i)!=u8(value>>shift)){return false;}i+=1u64;}return true;
}
bool runtime_symbol(SelectParams params,BindingBytes name) on cpu {u64 i=0u64;while(i<params.runtime_symbol_count){if(equal(name,name_at(params.runtime_symbols,i))){return true;}i+=1u64;}return false;}
bool exported(SelectView view,SelectObject object,BindingBytes name) on cpu {u64 i=object.export_start;while(i<object.export_start+object.export_count){if(equal(export_at(view,i).name,name)){return true;}i+=1u64;}return false;}
void refresh(inout SelectState state,SelectView view) on cpu {state.view=view;state.forest.nodes=view.nodes;}
SelectResult define(inout SelectState state,u64 object_index) on cpu {
    SelectObject object=object_at(state.view,object_index);u64 i=object.symbol_start;
    while(i<object.symbol_start+object.symbol_count){SelectSymbol symbol=symbol_at(state.view,i);BindingKey key=BindingKey(symbol.name,empty());
        if(find(state.forest,state.forest.names,key)!=0u64){return result(2u64,symbol.name,empty(),state.entry);}
        BindingInsert inserted=put(state.forest,state.forest.names,key,i+1u64);state.forest.names=inserted.root;i+=1u64;}
    return success(state.entry);
}
void enqueue(inout SelectState state,u64 section_index) on cpu {
    SelectSection row=section_at(state.view,section_index);if(row.live==0u64){row.live=1u64;section_put(state.view,section_index,row);index_put(state.view.pending,state.tail,section_index);state.tail+=1u64;}
}
void include(inout SelectState state,u64 object_index,u64 section) on cpu {
    SelectObject object=object_at(state.view,object_index);
    if(object.selected==0u64){object.selected=1u64;object_put(state.view,object_index,object);u64 i=object.section_start;
        while(i<object.section_start+object.section_count){if((section_at(state.view,i).flags&4096u64)==0u64){enqueue(state,i);}i+=1u64;}}
    enqueue(state,object.section_start+section);
}
SelectTarget resolve(inout SelectState state,BindingBytes name,BindingBytes contract,address context,cfn<SelectReply,address,u64,BindingBytes> extract) on cpu requires nonzero(extract) {
    u64 at=find(state.forest,state.forest.names,BindingKey(name,empty()));
    if(at==0u64){
        u64 library=0u64;u64 object_index=0u64;bool found=false;
        while(library<state.view.library_count){SelectReply reply=extract(context,library,name);refresh(state,reply.view);
            if(reply.error!=0u64){return SelectTarget(0u64,0u64,result(7u64,name,empty(),state.entry));}
            if(reply.object!=0u64){object_index=reply.object-1u64;found=true;break;}library+=1u64;}
        if(!found){return SelectTarget(0u64,0u64,result(3u64,name,empty(),state.entry));}
        SelectResult defined=define(state,object_index);if(defined.error!=0u64){return SelectTarget(0u64,0u64,defined);}
        at=find(state.forest,state.forest.names,BindingKey(name,empty()));
        if(at==0u64){return SelectTarget(0u64,0u64,result(5u64,name,object_at(state.view,object_index).name,state.entry));}
    }
    SelectSymbol symbol=symbol_at(state.view,node_at(state.forest,at).value-1u64);
    if(!equal(symbol.contract,contract)){return SelectTarget(0u64,0u64,result(4u64,name,empty(),state.entry));}
    return SelectTarget(symbol.owner,symbol.section,success(state.entry));
}
bool deferred(SelectState state,SelectParams params,BindingBytes name) on cpu {
    if(find(state.forest,state.forest.names,BindingKey(name,empty()))!=0u64){return false;}
    if(find(state.forest,state.forest.imports,BindingKey(name,empty()))!=0u64||runtime_symbol(params,name)){return true;}
    u64 i=0u64;while(i<state.view.object_count){SelectObject object=object_at(state.view,i);
        if(object.has_resources!=0u64&&(suffixed(name,object.prefix,7017276058679997535u64,25971u64,10u64)||suffixed(name,object.prefix,7881706611452047455u64,435727458911u64,13u64))){return true;}i+=1u64;}return false;
}
SelectImport required_import(SelectView view,SelectObject object,BindingBytes name) on cpu {
    u64 begin=object.import_start;u64 end=begin+object.import_count;while(begin<end){u64 middle=begin+(end-begin)/2u64;
        SelectImport row=import_row(view,middle);i32 order=bytes_compare(row.name,name);if(order==0){return row;}if(order<0){begin=middle+1u64;}else{end=middle;}}
    return SelectImport(empty(),empty());
}
void finish(inout SelectState state,SelectParams params) on cpu {
    u64 o=0u64;while(o<state.view.object_count){SelectObject object=object_at(state.view,o);
        if(object.selected!=0u64){
            u64 mapping=0u64;u64 i=object.section_start;
            while(i<object.section_start+object.section_count){SelectSection section=section_at(state.view,i);section.mapping=mapping;if(section.live!=0u64){mapping+=1u64;}section_put(state.view,i,section);i+=1u64;}
            object.keep_archive=object.archive;if(params.native_mode!=0u64||object.borrowed!=0u64){object.keep_archive=0u64;}
            if(object.kind==0u64&&object.keep_archive!=0u64){
                state.forest.count=0u64;state.forest.names=0u64;state.forest.imports=0u64;i=object.section_start;
                while(i<object.section_start+object.section_count){SelectSection section=section_at(state.view,i);if(section.live!=0u64){
                    u64 r=section.relocation_start;while(r<section.relocation_start+section.relocation_count){SelectRelocation relocation=relocation_at(state.view,r);
                        if(relocation.kind==2u64&&equal(relocation.dll,params.runtime_library)){BindingInsert item=put(state.forest,state.forest.names,BindingKey(relocation.name,empty()),0u64);state.forest.names=item.root;}r+=1u64;}}i+=1u64;}
                bool used=false;i=object.export_start;while(i<object.export_start+object.export_count){SelectExport row=export_at(state.view,i);row.keep=0u64;
                    if(find(state.forest,state.forest.names,BindingKey(row.name,empty()))!=0u64){row.keep=1u64;used=true;}
                    if(object.has_resources!=0u64&&suffixed(row.name,object.prefix,7165919078637728351u64,29541u64,10u64)){row.keep=1u64;}export_put(state.view,i,row);i+=1u64;}
                if(!used){object.keep_archive=0u64;}
            }
            i=object.section_start;while(i<object.section_start+object.section_count){SelectSection section=section_at(state.view,i);if(section.live!=0u64){
                u64 r=section.relocation_start;while(r<section.relocation_start+section.relocation_count){SelectRelocation row=relocation_at(state.view,r);
                    if(row.kind==0u64){row.section=section_at(state.view,object.section_start+row.section).mapping;relocation_put(state.view,r,row);}r+=1u64;}
                u64 a=section.associated_start;while(a<section.associated_start+section.associated_count){index_put(state.view.associated,a,section_at(state.view,object.section_start+index_at(state.view.associated,a)).mapping);a+=1u64;}}
                i+=1u64;
            }
            i=object.symbol_start;while(i<object.symbol_start+object.symbol_count){SelectSymbol symbol=symbol_at(state.view,i);SelectSection section=section_at(state.view,object.section_start+symbol.section);symbol.keep=section.live;symbol.section=section.mapping;symbol_put(state.view,i,symbol);i+=1u64;}
            i=object.frame_start;while(i<object.frame_start+object.frame_count){SelectFrame frame=frame_at(state.view,i);frame.keep=0u64;
                if(object.borrowed==0u64){if(frame.present==0u64){frame.keep=object.keep_archive;}else{SelectSection section=section_at(state.view,object.section_start+frame.section);frame.keep=section.live;frame.mapping=section.mapping;}}
                frame_put(state.view,i,frame);i+=1u64;}
            object_put(state.view,o,object);
        }o+=1u64;
    }
}
extern "C" SelectResult choose(SelectView input,SelectParams params,address context,
    cfn<SelectReply,address,u64,BindingBytes> extract,cfn<SelectReply,address,u64> native_archive,
    cfn<SelectReply,address,SelectResource> resources,cfn<BindingBytes,address,BindingBytes> slot) on cpu
    requires nonzero(extract) requires nonzero(native_archive) requires nonzero(resources) requires nonzero(slot) {
    SelectState state=SelectState(input,BindingForest(input.nodes,0u64,0u64,0u64),0u64,0u64,0u64,0u64,0u64);
    if(params.native_mode==0u64){u64 count=0u64;u64 i=0u64;while(i<input.object_count){if(object_at(input,i).kind!=0u64){state.entry=i;count+=1u64;}i+=1u64;}
        if(count!=1u64){return result(1u64,empty(),empty(),0u64);}}
    u64 o=0u64;while(o<state.view.object_count){SelectResult defined=define(state,ordered(o,state.entry));if(defined.error!=0u64){return defined;}o+=1u64;}
    if(params.native_mode==0u64){SelectObject entry=object_at(state.view,state.entry);
        if(entry.kind==2u64){entry.selected=1u64;object_put(state.view,state.entry,entry);u64 i=entry.section_start;while(i<entry.section_start+entry.section_count){enqueue(state,i);i+=1u64;}}
        else{include(state,state.entry,0u64);}
    }else{u64 i=0u64;while(i<params.root_count){SelectRoot root=root_at(params,i);SelectTarget target=resolve(state,root.name,root.contract,context,extract);if(target.result.error!=0u64){return target.result;}include(state,target.object,target.section);i+=1u64;}}
    bool bind=params.native_mode!=0u64;
    while(true){
        while(state.head<state.tail){u64 section_index=index_at(state.view.pending,state.head);state.head+=1u64;SelectSection section=section_at(state.view,section_index);u64 object_index=section.owner;SelectObject object=object_at(state.view,object_index);
            u64 a=section.associated_start;while(a<section.associated_start+section.associated_count){include(state,object_index,index_at(state.view.associated,a));a+=1u64;}
            u64 r=section.relocation_start;
            while(r<section.relocation_start+section.relocation_count){
                SelectRelocation relocation=relocation_at(state.view,r);
                if(relocation.kind==2u64&&equal(relocation.dll,params.runtime_library)&&object.archive!=0u64&&exported(state.view,object,relocation.name)){
                    u64 c=object.code_start;while(c<object.code_start+object.code_count){BindingBytes name=name_at(state.view.code,c);u64 at=find(state.forest,state.forest.names,BindingKey(name,empty()));
                        if(at==0u64){return result(6u64,name,empty(),state.entry);}SelectSymbol symbol=symbol_at(state.view,node_at(state.forest,at).value-1u64);
                        if(symbol.owner!=object_index){return result(6u64,name,empty(),state.entry);}include(state,object_index,symbol.section);c+=1u64;}
                    if(params.native_mode!=0u64){object=object_at(state.view,object_index);if(object.resources==0u64){SelectReply reply=native_archive(context,object_index);refresh(state,reply.view);if(reply.error!=0u64){return result(7u64,empty(),empty(),state.entry);}object.resources=1u64;object_put(state.view,object_index,object);}
                        SelectTarget target=resolve(state,relocation.name,empty(),context,extract);if(target.result.error!=0u64){return target.result;}include(state,target.object,target.section);
                        // All local slots use the same runtime DLL prefix, so name and slot order agree.
                        u64 local=state.forest.imports;u64 found=0u64;
                        while(local!=0u64){BindingNode node=node_at(state.forest,local);SelectLocal row=local_at(state.view,node.value-1u64);i32 order=bytes_compare(relocation.name,row.name);if(order==0){found=local;break;}if(order<0){local=node.left;}else{local=node.right;}}
                        u64 index=0u64;if(found==0u64){BindingBytes name=slot(context,relocation.name);index=state.local_count;state.local_count+=1u64;local_put(state.view,index,SelectLocal(name,relocation.name));BindingInsert item=put(state.forest,state.forest.imports,BindingKey(name,empty()),index+1u64);state.forest.imports=item.root;}
                        else{index=node_at(state.forest,found).value-1u64;}
                        relocation.rewrite=index+1u64;relocation_put(state.view,r,relocation);
                    }
                }
                if(relocation.kind==0u64){include(state,object_index,relocation.section);}
                else if(relocation.kind==1u64){if(params.native_mode==0u64||!deferred(state,params,relocation.name)){
                    SelectImport imported=required_import(state.view,object,relocation.name);if(imported.name.length==0u64){return result(8u64,relocation.name,object.name,state.entry);}
                    SelectTarget target=resolve(state,relocation.name,imported.contract,context,extract);if(target.result.error!=0u64){return target.result;}include(state,target.object,target.section);}}
                r+=1u64;
            }
        }
        if(!bind){break;}bind=false;u64 archive_count=0u64;o=0u64;
        while(o<state.view.object_count){u64 index=ordered(o,state.entry);SelectObject object=object_at(state.view,index);
            if(object.selected!=0u64){if(object.resources!=0u64){index_put(state.view.archives,archive_count,index);archive_count+=1u64;}u64 s=object.section_start;
                while(s<object.section_start+object.section_count){SelectSection section=section_at(state.view,s);if(section.live!=0u64){u64 r=section.relocation_start;
                    while(r<section.relocation_start+section.relocation_count){SelectRelocation relocation=relocation_at(state.view,r);if(relocation.kind==1u64&&runtime_symbol(params,relocation.name)){BindingInsert item=put(state.forest,state.runtime_root,BindingKey(relocation.name,empty()),0u64);state.runtime_root=item.root;}r+=1u64;}}s+=1u64;}}
            o+=1u64;
        }
        u64 runtime_count=0u64;u64 at=first(state.forest,state.runtime_root);
        while(at!=0u64){name_put(state.view.runtime,runtime_count,node_at(state.forest,at).key.first);runtime_count+=1u64;at=next(state.forest,at);}
        SelectReply reply=resources(context,SelectResource(state.view.archives,archive_count,state.view.runtime,runtime_count,state.view.locals,state.local_count,params.types,params.programs));refresh(state,reply.view);
        if(reply.error!=0u64){return result(7u64,empty(),empty(),state.entry);}
        if(reply.object!=0u64){u64 index=reply.object-1u64;SelectResult defined=define(state,index);if(defined.error!=0u64){return defined;}SelectObject object=object_at(state.view,index);object.selected=1u64;object_put(state.view,index,object);u64 s=object.section_start;while(s<object.section_start+object.section_count){enqueue(state,s);s+=1u64;}}
    }
    finish(state,params);return success(state.entry);
}
