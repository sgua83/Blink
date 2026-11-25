module bootstrap.type_value;

bool word<const N:u32 in {3,4,9}>(Text text, read array<u8,N> expected) on cpu {
    if (text.length != u64(N)) { return false; }
    for (index in 0..N) {
        if (unsafe_position<u8>(unsafe_address(address_bits(text.bytes)+u64(index))) != expected[index]) { return false; }
    }
    return true;
}
bool plain_void(Type value, bool canonical) on cpu {
    return value.arguments == 0u64 && value.flags == 0 && (!canonical || value.canonical == 0) && word(value.name,"void");
}
bool is_shape(Type value) on cpu {
    return address_bits(value.value) == 0u64 || (value.arguments == 0u64 && (value.flags & 7) == 0);
}

extern "C" Cursor type_start(Type value,BuiltinKind kind) on cpu {
    return Cursor(value,kind,Stage::Start,0,0u64,value,Record(0,0u64,0u64,0u64,0u64,0,0),0,Count(0,0));
}
Request request(address state,Cursor cursor,Query kind,Site site,Binding binding,u64 index) on cpu {
    unsafe_position<Cursor>(state)=cursor;
    return Request(kind,site,binding,index);
}
Request complete(Cursor cursor,Host host,address context,bool success) on cpu
    requires nonzero(host.view) {
    if(success&&cursor.wrap!=0){host.view(context);}
    return Request(Query::Complete,Site::Root,Binding::Representation,0u64);
}
extern "C" Request type_step(address state,Host host,address context) on cpu
    requires nonzero(host.failure) requires nonzero(host.builtin)
    requires nonzero(host.unit) requires nonzero(host.view) requires nonzero(host.argument) requires nonzero(host.cleanup)
    requires nonzero(host.owner) requires nonzero(host.enter_cleanup) requires nonzero(host.leave_cleanup) requires nonzero(host.owner_matches)
    requires nonzero(host.use_owner) requires nonzero(host.save_payload) requires nonzero(host.result) requires nonzero(host.array_size) requires nonzero(host.array)
    requires nonzero(host.record) requires nonzero(host.type_argument) requires nonzero(host.const_domain)
    requires nonzero(host.const_argument) requires nonzero(host.arguments_done) requires nonzero(host.enter_record)
    requires nonzero(host.leave_record) requires nonzero(host.simple_enum) requires nonzero(host.begin_variants)
    requires nonzero(host.unique_variant_name) requires nonzero(host.unique_variant_tag)
    requires nonzero(host.variant) requires nonzero(host.variants_done) requires nonzero(host.begin_record)
    requires nonzero(host.unique_field) requires nonzero(host.dataset_field) requires nonzero(host.reference_dataset) requires nonzero(host.field)
    requires nonzero(host.record_done) requires nonzero(host.stride_accepts)
    requires nonzero(host.set_stride) requires nonzero(host.use_record) requires nonzero(host.facts)
    requires nonzero(host.task) requires nonzero(host.views) {
    Cursor cursor=unsafe_position<Cursor>(state);
    bool starting=cursor.stage==Stage::Start;
    if(starting){
        Type value=cursor.value;
        if((value.flags&16)!=0&&cursor.kind!=BuiltinKind::CFunction){
            host.failure(context,Site::Root,0u64,Problem::Contracts,0u64);return complete(cursor,host,context,false);
        }
        if((value.flags&2)!=0){
            cursor.wrap=1;
            value.flags&=~2;
            cursor.value.flags=value.flags;
        }
        if(cursor.kind==BuiltinKind::CFunction&&(value.flags&7)==0){
            cursor.stage=Stage::FunctionDone;return request(state,cursor,Query::Function,Site::Root,Binding::Representation,0u64);
        }
        if(word(value.name,"texture2d")||word(value.name,"texture3d")){
            host.failure(context,Site::Root,0u64,Problem::Texture,0u64);return complete(cursor,host,context,false);
        }
        if(cursor.kind==BuiltinKind::Void){host.failure(context,Site::Root,0u64,Problem::Void,0u64);return complete(cursor,host,context,false);}
        if(cursor.kind==BuiltinKind::Never){host.failure(context,Site::Root,0u64,Problem::Never,0u64);return complete(cursor,host,context,false);}
        if((value.flags&4)!=0){host.failure(context,Site::Root,0u64,Problem::Tile,0u64);return complete(cursor,host,context,false);}
        if(cursor.kind==BuiltinKind::Numeric||cursor.kind==BuiltinKind::Address||cursor.kind==BuiltinKind::Handle||cursor.kind==BuiltinKind::RayHit){
            if(value.arguments!=0u64){host.failure(context,Site::Root,0u64,Problem::BuiltinArguments,0u64);return complete(cursor,host,context,false);}
            host.builtin(context);return complete(cursor,host,context,true);
        }
    }
    if(cursor.stage==Stage::FunctionDone){return complete(cursor,host,context,true);}

    // The kind is fixed while World answers this cursor's requests.
    if(cursor.kind==BuiltinKind::Task||cursor.kind==BuiltinKind::TaskStartError){
        if(starting){
            Type value=cursor.value;
            if(value.arguments!=1u64){host.failure(context,Site::Root,0u64,Problem::TaskArguments,0u64);return complete(cursor,host,context,false);}
            cursor.temporary=host.argument(value.value,0u64);
            if(address_bits(cursor.temporary.value)==0u64){host.failure(context,Site::Root,0u64,Problem::TaskArguments,0u64);return complete(cursor,host,context,false);}
            cursor.stage=Stage::TaskAfterChild;
            if(cursor.kind==BuiltinKind::Task&&plain_void(cursor.temporary,false)){
                host.unit(context);
            }else{
                return request(state,cursor,Query::Child,Site::Root,Binding::Representation,0u64);
            }
        }
        if(cursor.stage==Stage::TaskAfterChild){
            cursor.stage=Stage::TaskBuild;
            if((cursor.temporary.flags&1)!=0){return request(state,cursor,Query::Dataset,Site::Root,Binding::Representation,0u64);}
        }
        if(!host.task(context,cursor.kind==BuiltinKind::TaskStartError?1:0)){
            host.failure(context,Site::Root,0u64,Problem::TaskOverflow,0u64);return complete(cursor,host,context,false);
        }
        return complete(cursor,host,context,true);
    }

    if(cursor.kind==BuiltinKind::Owned){
        if(starting){
            Type value=cursor.value;
            if(value.arguments!=2u64){host.failure(context,Site::Root,0u64,Problem::OwnedArguments,0u64);return complete(cursor,host,context,false);}
            cursor.temporary=host.argument(value.value,0u64);
            Type release=host.argument(value.value,1u64);
            if(address_bits(cursor.temporary.value)==0u64||address_bits(release.value)==0u64){
                host.failure(context,Site::Root,0u64,Problem::OwnedArguments,0u64);return complete(cursor,host,context,false);
            }
            if((release.flags&7)!=0||release.arguments!=0u64){host.failure(context,Site::Root,0u64,Problem::ReleaseForm,0u64);return complete(cursor,host,context,false);}
            Cleanup cleanup=host.cleanup(context,release);
            if(cleanup.ready==0){return complete(cursor,host,context,false);}
            if(cleanup.ordinary==0||cleanup.cpu==0||cleanup.parameters!=1u64||cleanup.value_parameter==0){
                host.failure(context,Site::Root,0u64,Problem::ReleaseShape,0u64);return complete(cursor,host,context,false);
            }
            cursor.stage=Stage::OwnedAfterChild;return request(state,cursor,Query::Child,Site::Root,Binding::Representation,0u64);
        }
        if(cursor.stage==Stage::OwnedAfterChild){
            cursor.stage=Stage::OwnedSave;
            if((cursor.temporary.flags&1)!=0){
                if(host.views(context)){host.failure(context,Site::Root,0u64,Problem::DatasetView,0u64);return complete(cursor,host,context,false);}
                return request(state,cursor,Query::Dataset,Site::Root,Binding::Representation,0u64);
            }
        }
        if(cursor.stage==Stage::OwnedSave){
            host.owner(context);
            if(!host.enter_cleanup(context)){host.failure(context,Site::Root,0u64,Problem::OwnedCycle,0u64);return complete(cursor,host,context,false);}
            cursor.stage=Stage::OwnedBindResult;return request(state,cursor,Query::SpecializeCleanup,Site::Root,Binding::Representation,0u64);
        }
        if(cursor.stage==Stage::OwnedBindResult){
            cursor.stage=Stage::OwnedCheckResult;return request(state,cursor,Query::Bind,Site::Root,Binding::CleanupResult,0u64);
        }
        if(cursor.stage==Stage::OwnedCheckResult){
            if(!plain_void(cursor.temporary,true)){host.failure(context,Site::Root,0u64,Problem::ReleaseResult,0u64);return complete(cursor,host,context,false);}
            cursor.stage=Stage::OwnedParameter;return request(state,cursor,Query::Bind,Site::Root,Binding::CleanupParameter,0u64);
        }
        if(cursor.stage==Stage::OwnedParameter){
            cursor.stage=Stage::OwnedAfterParameter;return request(state,cursor,Query::Child,Site::CleanupParameter,Binding::Representation,0u64);
        }
        if(cursor.stage==Stage::OwnedAfterParameter){
            cursor.stage=Stage::OwnedFinish;
            if((cursor.temporary.flags&1)!=0){return request(state,cursor,Query::Dataset,Site::CleanupParameter,Binding::Representation,0u64);}
        }
        host.leave_cleanup(context);
        if(!host.owner_matches(context)){host.failure(context,Site::Root,0u64,Problem::ReleasePayload,0u64);return complete(cursor,host,context,false);}
        host.use_owner(context);return complete(cursor,host,context,true);
    }

    if(cursor.kind==BuiltinKind::Result){
        if(starting){
            Type value=cursor.value;
            if(value.arguments!=2u64){host.failure(context,Site::Root,0u64,Problem::ResultArguments,0u64);return complete(cursor,host,context,false);}
            cursor.index=0u64;cursor.stage=Stage::ResultItem;
        }
        while(true){
            if(cursor.stage==Stage::ResultItem){
                if(cursor.index==2u64){
                    if(!host.result(context)){host.failure(context,Site::Root,0u64,Problem::ResultOverflow,0u64);return complete(cursor,host,context,false);}
                    return complete(cursor,host,context,true);
                }
                cursor.temporary=host.argument(cursor.value.value,cursor.index);
                if(address_bits(cursor.temporary.value)==0u64){host.failure(context,Site::Root,0u64,Problem::ResultType,0u64);return complete(cursor,host,context,false);}
                if((cursor.temporary.flags&4)!=0){host.failure(context,Site::Root,0u64,Problem::ResultTile,0u64);return complete(cursor,host,context,false);}
                cursor.stage=Stage::ResultAfterChild;
                if(plain_void(cursor.temporary,true)){host.unit(context);continue;}
                return request(state,cursor,Query::Child,Site::Root,Binding::Representation,0u64);
            }
            if(cursor.stage==Stage::ResultAfterChild){
                cursor.stage=Stage::ResultSave;
                if((cursor.temporary.flags&1)!=0){
                    if(host.views(context)){host.failure(context,Site::Root,0u64,Problem::DatasetViewSpaced,0u64);return complete(cursor,host,context,false);}
                    return request(state,cursor,Query::Dataset,Site::Root,Binding::Representation,0u64);
                }
            }
            if(cursor.stage==Stage::ResultSave){host.save_payload(context);cursor.index+=1u64;cursor.stage=Stage::ResultItem;}
        }
    }

    if(cursor.kind==BuiltinKind::Array){
        if(starting){
            Type value=cursor.value;
            if(value.arguments!=2u64){host.failure(context,Site::Root,0u64,Problem::ArrayArguments,0u64);return complete(cursor,host,context,false);}
            cursor.temporary=host.argument(value.value,0u64);
            if(address_bits(cursor.temporary.value)==0u64){host.failure(context,Site::Root,0u64,Problem::ArrayType,0u64);return complete(cursor,host,context,false);}
            if((cursor.temporary.flags&4)!=0){host.failure(context,Site::Root,0u64,Problem::ArrayTile,0u64);return complete(cursor,host,context,false);}
            cursor.stage=Stage::ArrayAfterChild;return request(state,cursor,Query::Child,Site::Root,Binding::Representation,0u64);
        }
        if(cursor.stage==Stage::ArrayAfterChild){
            cursor.stage=Stage::ArrayCount;
            if((cursor.temporary.flags&1)!=0){
                if(host.views(context)){host.failure(context,Site::Root,0u64,Problem::DatasetViewSpaced,0u64);return complete(cursor,host,context,false);}
                return request(state,cursor,Query::Dataset,Site::Root,Binding::Representation,0u64);
            }
        }
        if(cursor.stage==Stage::ArrayCount){
            Type length=host.argument(cursor.value.value,1u64);
            if(!is_shape(length)){host.failure(context,Site::Root,0u64,Problem::ArrayLength,0u64);return complete(cursor,host,context,false);}
            cursor.stage=Stage::ArrayBuild;return request(state,cursor,Query::ShapeArgument,Site::Root,Binding::Representation,1u64);
        }
        u64 size=host.array_size(context,cursor.scalar);
        if(size>4294967295u64){host.failure(context,Site::Root,0u64,Problem::ArrayOverflow,0u64);return complete(cursor,host,context,false);}
        host.array(context,cursor.scalar,u32(size));
        return complete(cursor,host,context,true);
    }

    if(starting){
        cursor.record=host.record(context);
        if(cursor.record.ready==0){return complete(cursor,host,context,false);}
        if(cursor.record.types+cursor.record.values!=cursor.value.arguments){host.failure(context,Site::Root,0u64,Problem::RecordArguments,0u64);return complete(cursor,host,context,false);}
        cursor.index=0u64;cursor.stage=Stage::RecordTypeItem;
    }
    while(true){
        if(cursor.stage==Stage::RecordTypeItem){
            if(cursor.index==cursor.record.types){cursor.index=0u64;cursor.stage=Stage::RecordConstItem;continue;}
            cursor.temporary=host.argument(cursor.value.value,cursor.index);
            if(address_bits(cursor.temporary.value)==0u64){host.failure(context,Site::Root,0u64,Problem::RecordType,0u64);return complete(cursor,host,context,false);}
            if((cursor.temporary.flags&4)!=0){host.failure(context,Site::Root,0u64,Problem::RecordTile,0u64);return complete(cursor,host,context,false);}
            cursor.stage=Stage::RecordTypeAfterChild;
            if(plain_void(cursor.temporary,true)){host.unit(context);continue;}
            return request(state,cursor,Query::Child,Site::Root,Binding::Representation,0u64);
        }
        if(cursor.stage==Stage::RecordTypeAfterChild){
            cursor.stage=Stage::RecordTypeFamily;
            if((cursor.temporary.flags&1)!=0){
                if(host.views(context)){host.failure(context,Site::Root,0u64,Problem::DatasetView,0u64);return complete(cursor,host,context,false);}
                return request(state,cursor,Query::Dataset,Site::Root,Binding::Representation,0u64);
            }
        }
        if(cursor.stage==Stage::RecordTypeFamily){cursor.stage=Stage::RecordTypeAccepted;return request(state,cursor,Query::Family,Site::Root,Binding::Representation,cursor.index);}
        if(cursor.stage==Stage::RecordTypeAccepted){
            if(cursor.scalar==0){host.failure(context,Site::Root,cursor.index,Problem::Family,0u64);return complete(cursor,host,context,false);}
            host.type_argument(context,cursor.index);cursor.index+=1u64;cursor.stage=Stage::RecordTypeItem;
        }
        if(cursor.stage==Stage::RecordConstItem){
            if(cursor.index<cursor.record.values){
                Type argument=host.argument(cursor.value.value,cursor.record.types+cursor.index);
                if(!is_shape(argument)){host.failure(context,Site::Root,0u64,Problem::RecordValue,0u64);return complete(cursor,host,context,false);}
                cursor.stage=Stage::RecordConstStore;return request(state,cursor,Query::ShapeArgument,Site::Root,Binding::Representation,cursor.record.types+cursor.index);
            }
            host.arguments_done(context);
            if(!host.enter_record(context)){host.failure(context,Site::Root,0u64,Problem::RecordCycle,0u64);return complete(cursor,host,context,false);}
            if(cursor.record.enumeration!=0){cursor.stage=Stage::RecordRepresentation;return request(state,cursor,Query::Bind,Site::Root,Binding::Representation,0u64);}
            host.begin_record(context);cursor.index=0u64;cursor.stage=Stage::FieldItem;
        }
        if(cursor.stage==Stage::RecordConstStore){
            if(!host.const_domain(context,cursor.index,cursor.scalar)){host.failure(context,Site::Root,cursor.index,Problem::RecordDomain,u64(cursor.scalar));return complete(cursor,host,context,false);}
            host.const_argument(context,cursor.index,cursor.scalar);cursor.index+=1u64;cursor.stage=Stage::RecordConstItem;
        }
        if(cursor.stage==Stage::RecordRepresentation){cursor.stage=Stage::RecordRepresentationReady;return request(state,cursor,Query::Child,Site::Record,Binding::Representation,0u64);}
        if(cursor.stage==Stage::RecordRepresentationReady){
            Facts actual=host.facts(context);
            if(actual.numeric==0||actual.components!=1u64||actual.integer==0){host.failure(context,Site::Record,0u64,Problem::EnumScalar,0u64);return complete(cursor,host,context,false);}
            if(cursor.record.variants==0u64){host.simple_enum(context);host.leave_record(context);return complete(cursor,host,context,true);}
            if(!word(actual.name,"u32")&&!word(actual.name,"i32")){host.failure(context,Site::Record,0u64,Problem::VariantScalar,0u64);return complete(cursor,host,context,false);}
            host.begin_variants(context);cursor.index=0u64;cursor.stage=Stage::VariantItem;
        }
        if(cursor.stage==Stage::VariantItem){
            if(cursor.index==cursor.record.variants){
                if(!host.variants_done(context)){host.failure(context,Site::Record,0u64,Problem::VariantOverflow,0u64);return complete(cursor,host,context,false);}
                host.leave_record(context);host.use_record(context);return complete(cursor,host,context,true);
            }
            if(!host.unique_variant_name(context,cursor.index)){host.failure(context,Site::Variant,cursor.index,Problem::DuplicateVariantName,0u64);return complete(cursor,host,context,false);}
            cursor.stage=Stage::VariantTagged;return request(state,cursor,Query::VariantTag,Site::Variant,Binding::Representation,cursor.index);
        }
        if(cursor.stage==Stage::VariantTagged){
            if(!host.unique_variant_tag(context,cursor.scalar)){host.failure(context,Site::Variant,cursor.index,Problem::DuplicateTag,0u64);return complete(cursor,host,context,false);}
            cursor.stage=Stage::VariantPayload;return request(state,cursor,Query::Bind,Site::Variant,Binding::Variant,cursor.index);
        }
        if(cursor.stage==Stage::VariantPayload){
            if((cursor.temporary.flags&4)!=0){host.failure(context,Site::Variant,cursor.index,Problem::VariantTile,0u64);return complete(cursor,host,context,false);}
            cursor.stage=Stage::VariantAfterChild;
            if(plain_void(cursor.temporary,true)){host.unit(context);continue;}
            return request(state,cursor,Query::Child,Site::Variant,Binding::Representation,cursor.index);
        }
        if(cursor.stage==Stage::VariantAfterChild){
            cursor.stage=Stage::VariantStore;
            if((cursor.temporary.flags&1)!=0){
                if(host.views(context)){host.failure(context,Site::Variant,cursor.index,Problem::DatasetView,0u64);return complete(cursor,host,context,false);}
                return request(state,cursor,Query::Dataset,Site::Variant,Binding::Representation,cursor.index);
            }
        }
        if(cursor.stage==Stage::VariantStore){host.variant(context,cursor.index,cursor.scalar);cursor.index+=1u64;cursor.stage=Stage::VariantItem;}
        if(cursor.stage==Stage::FieldItem){
            if(cursor.index<cursor.record.fields){
                if(!host.unique_field(context,cursor.index)){host.failure(context,Site::Field,cursor.index,Problem::DuplicateField,0u64);return complete(cursor,host,context,false);}
                cursor.stage=Stage::FieldBound;return request(state,cursor,Query::Bind,Site::Field,Binding::Field,cursor.index);
            }
            if(!host.record_done(context)){host.failure(context,Site::Root,0u64,Problem::AlignedOverflow,0u64);return complete(cursor,host,context,false);}
            if(cursor.record.stride!=0){cursor.stage=Stage::RecordStride;return request(state,cursor,Query::RecordStride,Site::Record,Binding::Representation,0u64);}
            host.leave_record(context);host.use_record(context);return complete(cursor,host,context,true);
        }
        if(cursor.stage==Stage::FieldBound){
            if((cursor.temporary.flags&1)!=0){
                cursor.temporary=host.dataset_field(context);cursor.stage=Stage::FieldCount;
                return request(state,cursor,Query::Reference,Site::Field,Binding::Representation,cursor.index);
            }
            cursor.stage=Stage::FieldStore;return request(state,cursor,Query::Child,Site::Field,Binding::Representation,cursor.index);
        }
        if(cursor.stage==Stage::FieldCount){cursor.stage=Stage::FieldReference;return request(state,cursor,Query::FieldCount,Site::Field,Binding::Representation,cursor.index);}
        if(cursor.stage==Stage::FieldReference){host.reference_dataset(context,cursor.count);cursor.stage=Stage::FieldStore;}
        if(cursor.stage==Stage::FieldStore){
            if(!host.field(context,cursor.index)){host.failure(context,Site::Field,cursor.index,Problem::FieldOverflow,0u64);return complete(cursor,host,context,false);}
            cursor.index+=1u64;cursor.stage=Stage::FieldItem;
        }
        if(cursor.stage==Stage::RecordStride){
            if(!host.stride_accepts(context,cursor.scalar)){host.failure(context,Site::Record,0u64,Problem::Stride,u64(cursor.scalar));return complete(cursor,host,context,false);}
            host.set_stride(context,cursor.scalar);host.leave_record(context);host.use_record(context);return complete(cursor,host,context,true);
        }
    }
}
