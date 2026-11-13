module bootstrap.module_load;
import bootstrap.ast_storage;
import bootstrap.signatures;
import bootstrap.names;
import bootstrap.type_families;

address at(address base,u64 offset) on cpu {return unsafe_address(address_bits(base)+offset);}
T node<T>(ModuleView module,u64 offset) on cpu {
    return unsafe_position<T>(at(module.source.storage.nodes,offset));
}
u64 member(ModuleView module,Slice list,u64 index) on cpu {
    return node<u64>(module,list.offset+index*size_of<u64>());
}
NameBytes text(ModuleView module,Text name) on cpu {
    address bytes=module.source.storage.source;
    if(name.kind!=0){bytes=module.source.storage.text;}
    return NameBytes(at(bytes,name.range.start),name.range.end-name.range.start);
}
bool named<const N:u32>(NameBytes name,read array<u8,N> expected) on cpu {
    if(name.length!=u64(N)){return false;}
    for(index in 0..N){if(unsafe_position<u8>(at(name.bytes,u64(index)))!=expected[index]){return false;}}
    return true;
}
bool reserved(NameBytes name) on cpu {
    return named(name,array<u8,12>(u8(97),u8(99),u8(99),u8(101),u8(108),u8(101),u8(114),u8(97),u8(116),u8(105),u8(111),u8(110)))
        ||named(name,array<u8,7>(u8(100),u8(97),u8(116),u8(97),u8(115),u8(101),u8(116)))
        ||named(name,array<u8,4>(u8(118),u8(105),u8(101),u8(119)))
        ||named(name,array<u8,4>(u8(116),u8(105),u8(108),u8(101)))
        ||named(name,array<u8,5>(u8(97),u8(114),u8(114),u8(97),u8(121)))
        ||named(name,array<u8,6>(u8(114),u8(101),u8(115),u8(117),u8(108),u8(116)))
        ||named(name,array<u8,9>(u8(116),u8(101),u8(120),u8(116),u8(117),u8(114),u8(101),u8(50),u8(100)))
        ||named(name,array<u8,9>(u8(116),u8(101),u8(120),u8(116),u8(117),u8(114),u8(101),u8(51),u8(100)))
        ||named(name,array<u8,4>(u8(118),u8(111),u8(105),u8(100)))
        ||named(name,array<u8,5>(u8(110),u8(101),u8(118),u8(101),u8(114)));
}
bool intrinsic(NameBytes name) on cpu {
    BuiltinType builtin=recognize(name.bytes,name.length);
    return builtin.kind==BuiltinKind::Numeric||builtin.kind==BuiltinKind::Address
        ||builtin.kind==BuiltinKind::Handle||builtin.kind==BuiltinKind::RayHit;
}
bool family(NameBytes name) on cpu {
    return named(name,array<u8,7>(u8(110),u8(117),u8(109),u8(101),u8(114),u8(105),u8(99)))
        ||named(name,array<u8,8>(u8(102),u8(108),u8(111),u8(97),u8(116),u8(105),u8(110),u8(103)))
        ||named(name,array<u8,7>(u8(105),u8(110),u8(116),u8(101),u8(103),u8(101),u8(114)))
        ||named(name,array<u8,14>(u8(115),u8(99),u8(97),u8(108),u8(97),u8(114),u8(95),u8(110),u8(117),u8(109),u8(101),u8(114),u8(105),u8(99)))
        ||named(name,array<u8,14>(u8(115),u8(99),u8(97),u8(108),u8(97),u8(114),u8(95),u8(105),u8(110),u8(116),u8(101),u8(103),u8(101),u8(114)));
}

bool load_records(ModuleView module,address context,
    cfn<bool,address,NameBytes,u32> prepare,
    cfn<void,address,address,u64,u32> insert_type,
    cfn<void,address,LoadFailure,address,Span,NameBytes,NameBytes> report) on cpu
    requires nonzero(prepare) requires nonzero(insert_type) requires nonzero(report) {
    NameBytes empty=NameBytes(unsafe_address(0u64),0u64);
    ModuleNode declarations=node<ModuleNode>(module,module.source.offset);
    u64 index=0u64;
    while(index<declarations.records.count){
        u64 offset=member(module,declarations.records,index);
        RecordNode record=node<RecordNode>(module,offset);
        NameBytes record_name=text(module,record.name);
        if(reserved(record_name)||intrinsic(record_name)){
            report(context,LoadFailure::ReservedRecord,module.owner,record.span,empty,empty);return false;
        }
        if(prepare(context,record_name,0)){
            report(context,LoadFailure::DuplicateRecord,module.owner,record.span,empty,empty);return false;
        }
        insert_type(context,module.owner,offset,0);
        index+=1u64;
    }
    return true;
}

bool load_sets(ModuleView module,address context,
    cfn<bool,address,NameBytes,u32> prepare,
    cfn<void,address,address,u64,u32> insert_type,
    cfn<void,address,LoadFailure,address,Span,NameBytes,NameBytes> report) on cpu
    requires nonzero(prepare) requires nonzero(insert_type) requires nonzero(report) {
    NameBytes empty=NameBytes(unsafe_address(0u64),0u64);
    ModuleNode declarations=node<ModuleNode>(module,module.source.offset);
    u64 index=0u64;
    while(index<declarations.type_sets.count){
        u64 offset=member(module,declarations.type_sets,index);
        TypeSetNode set=node<TypeSetNode>(module,offset);
        NameBytes set_name=text(module,set.name);
        if(prepare(context,set_name,1)||intrinsic(set_name)||reserved(set_name)||family(set_name)){
            report(context,LoadFailure::DuplicateSet,module.owner,set.span,empty,empty);return false;
        }
        insert_type(context,module.owner,offset,1);
        index+=1u64;
    }
    return true;
}

bool load_functions(ModuleView module,ModuleView interface_module,u32 implementation,u32 compiled,
    address pairs,address context,
    cfn<bool,address,NameBytes,u32> prepare,
    cfn<u32,address> requirement,
    cfn<void,address,address,u64,Optional<FunctionRef>,u32,u32,u32> insert_function,
    cfn<void,address,LoadFailure,address,Span,NameBytes,NameBytes> report) on cpu
    requires nonzero(prepare) requires nonzero(requirement)
    requires nonzero(insert_function) requires nonzero(report) {
    NameBytes empty=NameBytes(unsafe_address(0u64),0u64);
    ModuleNode interface=node<ModuleNode>(interface_module,interface_module.source.offset);
    ModuleNode declarations=node<ModuleNode>(module,module.source.offset);
    u64 index=0u64;
    while(index<declarations.functions.count){
        u64 offset=member(module,declarations.functions,index);
        FunctionNode function=node<FunctionNode>(module,offset);
        if(implementation!=0||function.kind==2||function.external.present!=0||compiled!=0){
            if(function.body.present==0&&function.external.present==0&&compiled==0){
                report(context,LoadFailure::ImplementationBody,module.owner,function.span,empty,empty);return false;
            }
            NameBytes function_name=text(module,function.name);
            if(prepare(context,function_name,2)){
                report(context,LoadFailure::DuplicateValue,module.owner,function.span,empty,empty);return false;
            }
            u32 execution=0;
            if(compiled!=0){
                execution=requirement(context);
                if(execution==3){report(context,LoadFailure::MissingRequirement,module.owner,function.span,function_name,empty);return false;}
            }
            Optional<FunctionRef> declaration=Optional<FunctionRef>(0,FunctionRef(0u64));
            u32 public=1;
            if(implementation!=0){
                public=0;
                if(address_bits(pairs)!=0u64){
                    u64 paired_index=unsafe_position<u64>(at(pairs,(interface.functions.count+index)*size_of<u64>()));
                    if(paired_index<interface.functions.count){
                        declaration=Optional<FunctionRef>(1,FunctionRef(member(interface_module,interface.functions,paired_index)));
                        public=1;
                    }
                }
            }
            u32 link_body=u32(function.kind!=2&&function.external.present==0);
            insert_function(context,module.owner,offset,declaration,public,link_body,execution);
        }
        index+=1u64;
    }
    return true;
}

extern "C" address provider(address loaded,address compiled,address source) on cpu {
    if(address_bits(loaded)!=0u64){return loaded;}
    if(address_bits(compiled)!=0u64){return compiled;}
    return source;
}

extern "C" NameBytes load(LoadInput input,address context,
    cfn<bool,address> contains,
    cfn<void,address> enter,
    cfn<ImportList,address> imports,
    cfn<Imported,address,NameBytes> import_module,
    cfn<bool,address,NameBytes,u32> prepare,
    cfn<void,address,address,u64,u32> insert_type,
    cfn<address,address> pair,
    cfn<bool,address,address,u64,u64> signature,
    cfn<u32,address> requirement,
    cfn<void,address,address,u64,Optional<FunctionRef>,u32,u32,u32> insert_function,
    cfn<void,address,LoadFailure,address,Span,NameBytes,NameBytes> report) on cpu
    requires nonzero(contains) requires nonzero(enter) requires nonzero(imports)
    requires nonzero(import_module) requires nonzero(prepare) requires nonzero(insert_type)
    requires nonzero(pair) requires nonzero(signature) requires nonzero(requirement)
    requires nonzero(insert_function) requires nonzero(report) {
    NameBytes empty=NameBytes(unsafe_address(0u64),0u64);
    ModuleNode interface=node<ModuleNode>(input.interface,input.interface.source.offset);
    NameBytes name=text(input.interface,interface.name);
    address source_owner=input.interface.owner;
    if(input.has_implementation!=0){
        source_owner=input.implementation.owner;
        ModuleNode body=node<ModuleNode>(input.implementation,input.implementation.source.offset);
        if(compare_name(text(input.implementation,body.name),name)!=0){
            report(context,LoadFailure::ModuleName,source_owner,Span(0u64,0u64),empty,empty);return empty;
        }
    }
    if(contains(context)){return name;}
    enter(context);
    ImportList imported_names=imports(context);
    u64 imported_index=0u64;
    while(imported_index<imported_names.count){
        NameBytes imported_name=unsafe_position<NameBytes>(at(imported_names.names,imported_index*size_of<NameBytes>()));
        Imported imported=import_module(context,imported_name);
        if(imported.success==0){return empty;}
        if(compare_name(imported.name,imported_name)!=0){
            report(context,LoadFailure::ImportName,source_owner,Span(0u64,0u64),imported_name,imported.name);return empty;
        }
        imported_index+=1u64;
    }
    if(!load_records(input.interface,context,prepare,insert_type,report)){return empty;}
    if(input.has_implementation!=0){
        if(!load_records(input.implementation,context,prepare,insert_type,report)){return empty;}
    }
    if(!load_sets(input.interface,context,prepare,insert_type,report)){return empty;}
    if(input.has_implementation!=0){
        if(!load_sets(input.implementation,context,prepare,insert_type,report)){return empty;}
    }
    address pairs=pair(context);
    u64 implementation_count=0u64;
    if(input.has_implementation!=0){
        ModuleNode implementation=node<ModuleNode>(input.implementation,input.implementation.source.offset);
        implementation_count=implementation.functions.count;
    }
    u64 declaration_index=0u64;
    while(declaration_index<interface.functions.count){
        u64 offset=member(input.interface,interface.functions,declaration_index);
        FunctionNode function=node<FunctionNode>(input.interface,offset);
        if(function.kind!=2&&function.external.present==0){
            NameBytes function_name=text(input.interface,function.name);
            if(function.body.present!=0){
                report(context,LoadFailure::InterfaceBody,input.interface.owner,function.span,empty,empty);return empty;
            }
            if(input.compiled==0){
                u64 paired_index=implementation_count;
                if(address_bits(pairs)!=0u64){paired_index=unsafe_position<u64>(at(pairs,declaration_index*size_of<u64>()));}
                if(paired_index>=implementation_count){
                    report(context,LoadFailure::MissingBody,input.interface.owner,function.span,function_name,empty);return empty;
                }
                ModuleNode implementation=node<ModuleNode>(input.implementation,input.implementation.source.offset);
                u64 body_offset=member(input.implementation,implementation.functions,paired_index);
                if(!signature(context,input.implementation.owner,offset,body_offset)){
                    FunctionNode body=node<FunctionNode>(input.implementation,body_offset);
                    report(context,LoadFailure::Signature,input.implementation.owner,body.span,function_name,empty);return empty;
                }
            }
        }
        declaration_index+=1u64;
    }
    if(!load_functions(input.interface,input.interface,0,input.compiled,pairs,context,
        prepare,requirement,insert_function,report)){return empty;}
    if(input.has_implementation!=0){
        if(!load_functions(input.implementation,input.interface,1,input.compiled,pairs,context,
            prepare,requirement,insert_function,report)){return empty;}
    }
    return name;
}
