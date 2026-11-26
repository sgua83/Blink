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

extern "C" void load(LoadInput input,address context,
    cfn<bool,address> contains,
    cfn<void,address> enter,
    cfn<u64,address> imports,
    cfn<Imported,address,u64> import_module,
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
            report(context,LoadFailure::ModuleName,source_owner,Span(0u64,0u64),empty,empty);return;
        }
    }
    if(contains(context)){return;}
    enter(context);
    u64 import_count=imports(context);
    u64 imported_index=0u64;
    while(imported_index<import_count){
        Imported imported=import_module(context,imported_index);
        if(imported.success==0){return;}
        if(compare_name(imported.name,imported.requested)!=0){
            report(context,LoadFailure::ImportName,source_owner,Span(0u64,0u64),imported.requested,imported.name);return;
        }
        imported_index+=1u64;
    }
    if(!load_records(input.interface,context,prepare,insert_type,report)){return;}
    if(input.has_implementation!=0){
        if(!load_records(input.implementation,context,prepare,insert_type,report)){return;}
    }
    if(!load_sets(input.interface,context,prepare,insert_type,report)){return;}
    if(input.has_implementation!=0){
        if(!load_sets(input.implementation,context,prepare,insert_type,report)){return;}
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
                report(context,LoadFailure::InterfaceBody,input.interface.owner,function.span,empty,empty);return;
            }
            if(input.compiled==0){
                u64 paired_index=implementation_count;
                if(address_bits(pairs)!=0u64){paired_index=unsafe_position<u64>(at(pairs,declaration_index*size_of<u64>()));}
                if(paired_index>=implementation_count){
                    report(context,LoadFailure::MissingBody,input.interface.owner,function.span,function_name,empty);return;
                }
                ModuleNode implementation=node<ModuleNode>(input.implementation,input.implementation.source.offset);
                u64 body_offset=member(input.implementation,implementation.functions,paired_index);
                if(!signature(context,input.implementation.owner,offset,body_offset)){
                    FunctionNode body=node<FunctionNode>(input.implementation,body_offset);
                    report(context,LoadFailure::Signature,input.implementation.owner,body.span,function_name,empty);return;
                }
            }
        }
        declaration_index+=1u64;
    }
    if(!load_functions(input.interface,input.interface,0,input.compiled,pairs,context,
        prepare,requirement,insert_function,report)){return;}
    if(input.has_implementation!=0){
        if(!load_functions(input.implementation,input.interface,1,input.compiled,pairs,context,
            prepare,requirement,insert_function,report)){return;}
    }
}

bool source_definition(FunctionNode definition) on cpu {
    return definition.kind != 2 && definition.external.present == 0;
}

extern "C" ModulePlan native_module_plan(SignatureNode source,u32 compiled,u32 local,u32 domain,u32 exported,u32 explicit) on cpu {
    if (local == 0 || (explicit == 0 ? compiled != 0 : exported == 0)) {
        return ModulePlan(ModuleEntry::Native,0,0,0);
    }
    FunctionNode definition = unsafe_position<FunctionNode>(at(source.storage.nodes,source.offset));
    if (explicit == 0 && !source_definition(definition)) {
        return ModulePlan(ModuleEntry::Native,0,0,0);
    }
    if (definition.kind == 0 && definition.target != 2 && (explicit == 0 || definition.external.present == 0)) {

        return ModulePlan(ModuleEntry::Native,domain,exported,1);
    }
    if (explicit != 0) { return ModulePlan(ModuleEntry::Reject,domain,exported,1); }
    if (domain != 0) { return ModulePlan(ModuleEntry::DomainProgram,domain,exported,1); }
    return ModulePlan(ModuleEntry::Check,0,0,1);
}

extern "C" u32 source_module_entry(SignatureNode source,u32 local,u32 exported) on cpu {
    FunctionNode definition = unsafe_position<FunctionNode>(at(source.storage.nodes,source.offset));
    return local != 0 && exported != 0 && definition.kind == 0 ? 1 : 0;
}

SourcePlan source_plan(SourceCandidate candidate,SourceMode mode,bool public_root) on cpu {
    if (candidate.compiled != 0 || candidate.selected != 0) {
        return SourcePlan(SourceRoute::Function,0,0);
    }
    FunctionNode definition = unsafe_position<FunctionNode>(at(candidate.source.storage.nodes,candidate.source.offset));
    if (!source_definition(definition)) { return SourcePlan(SourceRoute::Function,0,0); }
    if (candidate.domain == 0) { return SourcePlan(SourceRoute::Function,1,0); }
    SourceRoute route = SourceRoute::ValidateProgram;
    if (mode == SourceMode::Application) { route = SourceRoute::CheckDomain; }
    else if (mode == SourceMode::Automatic && public_root) { route = SourceRoute::Program; }
    return SourcePlan(route,1,definition.target == 1 ? 1 : 0);
}

extern "C" u32 plan_source_module(SourceMode mode,SourcePlanningHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.is_open) requires nonzero(host.schedule)
    requires nonzero(host.record_open) requires nonzero(host.selected) {
    u32 open_public_domain = 0;
    while (true) {
        SourceCandidate candidate = host.next(host.context);
        if (address_bits(candidate.key) == 0u64) { break; }
        bool public_root = candidate.local != 0 && candidate.exported != 0;
        SourcePlan plan = source_plan(candidate,mode,public_root);
        bool open = false;
        if (plan.schedule != 0) {
            SourceExpansion expansion = host.schedule(host.context,candidate.key,candidate.definition,plan.route,plan.cpu_signature);
            if (expansion == SourceExpansion::Failed) { return open_public_domain; }
            open = expansion == SourceExpansion::Open;
        } else {
            open = host.is_open(host.context,candidate.definition) != 0;
        }
        if (open) {
            host.record_open(host.context,candidate.key);
            if (public_root && candidate.domain != 0) { open_public_domain = 1; }
        }
    }
    if (mode == SourceMode::SelectedProgram) { host.selected(host.context,SourceRoute::Program); }
    else if (mode == SourceMode::Application) { host.selected(host.context,SourceRoute::Application); }
    return open_public_domain;
}

extern "C" void compile_source_instance(SourceRoute route,u32 cpu_signature,SourceInstanceHost host) on cpu
    requires nonzero(host.signature) requires nonzero(host.function) requires nonzero(host.domain)
    requires nonzero(host.build_program) requires nonzero(host.application) {
    if (cpu_signature != 0) {
        SourceSignature signature = host.signature(host.context);
        if (signature == SourceSignature::Failed) { return; }
        if (signature == SourceSignature::Absent) {
            host.function(host.context);
            return;
        }
    }
    if (route == SourceRoute::Function) { host.function(host.context); }
    else if (route == SourceRoute::CheckDomain) { host.domain(host.context); }
    else if (route == SourceRoute::Application) { host.application(host.context); }
    else { host.build_program(host.context,route == SourceRoute::Program ? 1 : 0); }
}

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap,u32 flags,usize bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap,u32 flags,address rows) on cpu from "KERNEL32.dll";

const u32 CHECK_CONSTANT=1;
const u32 CHECK_INTERFACE=2;
const u32 CHECK_EXTERNAL=4;

struct PendingCheck { WorldFunction function; u32 kinds; };

address pending_at(address rows,u64 index) on cpu {
    return at(rows,index*size_of<PendingCheck>());
}

u32 required_checks(WorldDeclaration candidate) on cpu {
    let source=candidate.source;
    let declaration=unsafe_position<FunctionNode>(at(source.storage.nodes,source.offset));
    u32 kinds=0;
    if (declaration.kind==2) { kinds|=CHECK_CONSTANT; }
    if (declaration.types.count==0u64 && declaration.values.count==0u64) {
        if (candidate.paired!=0) { kinds|=CHECK_INTERFACE; }
        if (declaration.c_abi!=0) { kinds|=CHECK_EXTERNAL; }
    }
    return kinds;
}

bool run_pending(WorldPreparation host,address rows,u64 count,u32 mask,WorldCheck check) on cpu
    requires nonzero(host.run) {
    u64 index=0u64;
    while (index<count) {
        PendingCheck item=unsafe_position<PendingCheck>(pending_at(rows,index));
        if ((item.kinds&mask)!=0 && host.run(host.context,check,item.function)==0) { return false; }
        index+=1u64;
    }
    return true;
}

void run_world_checks(WorldPreparation host,address rows,u64 count) on cpu
    requires nonzero(host.track) requires nonzero(host.finish_interface) requires nonzero(host.run) {
    host.track(host.context,1);
    if (!run_pending(host,rows,count,CHECK_CONSTANT,WorldCheck::Constant)) { return; }
    host.track(host.context,0);
    bool interfaces=run_pending(host,rows,count,CHECK_INTERFACE,WorldCheck::Interface);
    host.finish_interface(host.context);
    if (!interfaces) { return; }
    if (host.run(host.context,WorldCheck::Types,WorldFunction(address(),address()))==0) { return; }
    run_pending(host,rows,count,CHECK_EXTERNAL,WorldCheck::External);
}

extern "C" u64 prepare_world(WorldPreparation host) on cpu
    requires nonzero(host.next) requires nonzero(host.signature)
    requires nonzero(host.track) requires nonzero(host.finish_interface) requires nonzero(host.run) {
    address rows=address();
    u64 count=0u64;
    bool valid=true;
    while (true) {
        let candidate=host.next(host.context);
        if (candidate.present==0) { break; }
        if (host.signature(host.context,candidate.function)==0) { valid=false; break; }
        u32 kinds=required_checks(candidate);
        if (kinds==0) { continue; }
        if (address_bits(rows)==0u64) {
            u64 bytes=host.declarations*size_of<PendingCheck>();
            rows=HeapAlloc(GetProcessHeap(),0,bytes);
            if (address_bits(rows)==0u64) { return bytes; }
        }
        unsafe_position<PendingCheck>(pending_at(rows,count))=PendingCheck(candidate.function,kinds);
        count+=1u64;
    }
    if (valid) { run_world_checks(host,rows,count); }
    if (address_bits(rows)!=0u64) { HeapFree(GetProcessHeap(),0,rows); }
    return 0u64;
}

extern "C" u32 load_export_types(StorageView storage,Slice arguments,ExportTypeHost host) on cpu
    requires nonzero(host.save) requires nonzero(host.resume) requires nonzero(host.load) {
    let syntax=ModuleView(address(),SignatureNode(storage,0u64));
    ExportRange active=ExportRange(arguments,arguments.count);
    while (true) {
        if (active.remaining==0u64) {
            active=host.resume(host.context);
            if (active.remaining==0u64) { return 1; }
        }
        active.remaining-=1u64;
        let argument=node<ArgumentNode>(syntax,member(syntax,active.arguments,active.remaining));
        match (argument) {
            ArgumentNode::Type(reference) {
                let named_type=node<TypeNode>(syntax,reference.offset);
                let name=text(syntax,named_type.name);
                let owner_end=qualification_separator(name);
                if (owner_end<name.length && !host.load(host.context,NameBytes(name.bytes,owner_end))) { return 0; }
                if (named_type.arguments.count!=0u64) {
                    if (active.remaining!=0u64) { host.save(host.context,active); }
                    active=ExportRange(named_type.arguments,named_type.arguments.count);
                }
            }
            ArgumentNode::Value(value) {}
        }
    }
}

bool capture_branch(CaptureHost host, NameBytes name) on cpu
    requires nonzero(host.known) requires nonzero(host.enter)
    requires nonzero(host.open) requires nonzero(host.bind)
    requires nonzero(host.name) requires nonzero(host.close) {
    CaptureOpened opened = host.open(host.context, name);
    if (opened.status == CaptureStatus::Failed) { return false; }
    if (opened.status == CaptureStatus::Ignored) { return true; }
    u32 include_imports = host.enter(host.context, opened.name);
    CaptureImports imports = host.bind(host.context, name, include_imports);
    if (imports.count == 0u64) { return true; }
    usize index = 0u64;
    bool complete = true;
    while (index < imports.count && complete) {
        NameBytes next = host.name(imports, index);
        if (host.known(host.context, next) == 0) { complete = capture_branch(host, next); }
        index += 1u64;
    }
    host.close(imports);
    return complete;
}

extern "C" void capture_build_inputs(CaptureHost host, usize requests) on cpu
    requires nonzero(host.known) requires nonzero(host.enter)
    requires nonzero(host.open) requires nonzero(host.bind)
    requires nonzero(host.name) requires nonzero(host.close) requires nonzero(host.export) {
    if (!capture_branch(host, NameBytes(unsafe_address(0u64), 0u64))) { return; }
    usize index = 0u64;
    while (index < requests) {
        if (host.export(host.context, index) == 0) { break; }
        index += 1u64;
    }
}

extern "C" u32 capture_export_owner(CaptureHost host, NameBytes name) on cpu
    requires nonzero(host.known) requires nonzero(host.enter)
    requires nonzero(host.open) requires nonzero(host.bind)
    requires nonzero(host.name) requires nonzero(host.close) requires nonzero(host.provider) {
    if (host.provider(host.context, name) == 0 || host.known(host.context, name) != 0) { return 1; }
    return capture_branch(host, name) ? 2 : 0;
}
