module bootstrap.generic_instance;

address at(address data,u64 offset) on cpu { return unsafe_address(address_bits(data)+offset); }
T row<T>(StorageView storage,u64 offset) on cpu { return unsafe_position<T>(at(storage.nodes,offset)); }
u64 item(StorageView storage,Slice values,u64 index) on cpu { return row<u64>(storage,values.offset+index*8u64); }
SignatureNode child(SignatureNode parent,u64 offset) on cpu { return SignatureNode(parent.storage,offset); }
NameBytes text(StorageView storage,Text value) on cpu {
    return NameBytes(at(value.kind==0 ? storage.source : storage.text,value.range.start),value.range.end-value.range.start);
}
bool same(NameBytes left,NameBytes right) on cpu {
    if(left.length!=right.length){return false;}
    u64 index=0u64;
    while(index<left.length){
        if(unsafe_position<u8>(at(left.bytes,index))!=unsafe_position<u8>(at(right.bytes,index))){return false;}
        index+=1u64;
    }
    return true;
}
bool word<const N:u32 in {3,4,5,6}>(NameBytes name,read array<u8,N> expected) on cpu {
    if(name.length!=u64(N)){return false;}
    for(index in 0..N){if(unsafe_position<u8>(at(name.bytes,u64(index)))!=expected[index]){return false;}}
    return true;
}
bool plain_void(SemanticInfo value) on cpu {return value.arguments==0u64&&value.flags==0&&word(value.name,"void");}
bool is_shape(SemanticInfo value) on cpu {return address_bits(value.value)==0u64||(value.arguments==0u64&&(value.flags&7)==0);}
u64 smaller(u64 left,u64 right) on cpu {return left<right ? left : right;}
TypeInfo absent_type() on cpu {return TypeInfo(address(),NameBytes(address(),0u64),0u64,address(),0u64,0u64,0);}
SemanticInfo absent_semantic() on cpu {return SemanticInfo(address(),NameBytes(address(),0u64),0u64,0,0);}
Need give_special(address state,SpecializeCursor cursor,Need need) on cpu {unsafe_position<SpecializeCursor>(state)=cursor;return need;}
Need give_match(address state,MatchCursor cursor,Need need) on cpu {unsafe_position<MatchCursor>(state)=cursor;return need;}
Need give_instance(address state,InstanceCursor cursor,Need need) on cpu {unsafe_position<InstanceCursor>(state)=cursor;return need;}
Need child_need(MatchMode mode) on cpu {return mode==MatchMode::Type ? Need::TypeChild : Need::ValueChild;}

extern "C" SpecializeCursor specialize_start(SignatureNode function,u64 actual_count,Optional<u64> explicit_count) on cpu {
    return SpecializeCursor(function,0,actual_count,explicit_count,0,0u64,0u64,0u64,
        NameBytes(address(),0u64),Optional<u32>(0,0),0,0,absent_semantic(),function,absent_type());
}
extern "C" Need specialize_step(address state,Host host,address context) on cpu
    requires nonzero(host.fail) requires nonzero(host.actual) requires nonzero(host.explicit_argument)
    requires nonzero(host.has_type) requires nonzero(host.push_type) requires nonzero(host.push_value) {
    SpecializeCursor cursor=unsafe_position<SpecializeCursor>(state);
    FunctionNode declaration=row<FunctionNode>(cursor.function.storage,cursor.function.offset);
    while(true){
        if(cursor.phase==0){
            NameBytes function_name=text(cursor.function.storage,declaration.name);
            if(declaration.types.count==0u64&&declaration.values.count==0u64&&cursor.explicit_count.present==0){
                cursor.phase=10;return give_special(state,cursor,Need::Instantiate);
            }
            if(cursor.explicit_count.present!=0){
                if(cursor.explicit_count.value==declaration.types.count+declaration.values.count&&cursor.explicit_count.value!=0u64){
                    cursor.explicit_types=1;cursor.first_value=declaration.types.count;
                }else if(cursor.explicit_count.value!=declaration.values.count||declaration.values.count==0u64){
                    host.fail(context,Problem::GenericArguments,function_name,address(),0,0);return Need::Done;
                }
            }
            cursor.index=0u64;cursor.phase=1;
        }
        if(cursor.phase==1){
            if(cursor.index==declaration.types.count){cursor.index=0u64;cursor.phase=6;continue;}
            TypeParameterNode parameter=row<TypeParameterNode>(cursor.function.storage,item(cursor.function.storage,declaration.types,cursor.index));
            cursor.name=text(cursor.function.storage,parameter.name);
            if(cursor.explicit_types!=0){
                cursor.semantic=host.explicit_argument(context,cursor.index);
                if(address_bits(cursor.semantic.value)==0u64){host.fail(context,Problem::TypePosition,cursor.name,address(),0,0);return Need::Done;}
                if((cursor.semantic.flags&6)!=0){host.fail(context,Problem::TypeWrapped,cursor.name,address(),0,0);return Need::Done;}
                cursor.unit=plain_void(cursor.semantic) ? 1 : 0;
                cursor.phase=2;return give_special(state,cursor,Need::ExplicitType);
            }
            cursor.position=0u64;cursor.phase=5;
        }
        if(cursor.phase==2){
            cursor.phase=3;
            if((cursor.semantic.flags&1)!=0){return give_special(state,cursor,Need::ExplicitDataset);}
        }
        if(cursor.phase==3){
            TypeParameterNode parameter=row<TypeParameterNode>(cursor.function.storage,item(cursor.function.storage,declaration.types,cursor.index));
            cursor.phase=4;cursor.scalar=1;
            if(parameter.family.present!=0){return give_special(state,cursor,Need::FamilyCurrent);}
        }
        if(cursor.phase==4){
            if(cursor.scalar==0){host.fail(context,Problem::ExplicitFamily,cursor.name,address(),0,0);return Need::Done;}
            host.push_type(context);cursor.index+=1u64;cursor.phase=1;continue;
        }
        if(cursor.phase==5){
            if(cursor.position==smaller(declaration.params.count,cursor.actual_count)){
                if(!host.has_type(context)){host.fail(context,Problem::MissingType,cursor.name,address(),0,0);return Need::Done;}
                host.push_type(context);cursor.index+=1u64;cursor.phase=1;continue;
            }
            ParamNode formal=row<ParamNode>(cursor.function.storage,item(cursor.function.storage,declaration.params,cursor.position));
            SignatureNode source=child(cursor.function,formal.ty.offset);
            cursor.position+=1u64;
            if(type_mentions(source,cursor.name.bytes,cursor.name.length)){
                ActualArgument actual=host.actual(context,cursor.position-1u64);
                if(address_bits(actual.ty.value)==0u64){host.fail(context,Problem::Resource,cursor.name,address(),0,0);return Need::Done;}
                cursor.next=source;cursor.next_actual=actual.ty;
                return give_special(state,cursor,Need::TypeChild);
            }
            continue;
        }
        if(cursor.phase==6){
            if(cursor.index==declaration.values.count){cursor.phase=10;return give_special(state,cursor,Need::Instantiate);}
            ValueParameterNode parameter=row<ValueParameterNode>(cursor.function.storage,item(cursor.function.storage,declaration.values,cursor.index));
            cursor.name=text(cursor.function.storage,parameter.name);cursor.inferred=Optional<u32>(0,0);
            if(cursor.explicit_count.present!=0){
                cursor.semantic=host.explicit_argument(context,cursor.first_value+cursor.index);
                if(!is_shape(cursor.semantic)){host.fail(context,Problem::ConstShape,cursor.name,address(),0,0);return Need::Done;}
                cursor.phase=7;return give_special(state,cursor,Need::ExplicitValue);
            }
            cursor.position=0u64;cursor.phase=8;
        }
        if(cursor.phase==7){cursor.phase=9;}
        if(cursor.phase==8){
            if(cursor.position==smaller(declaration.params.count,cursor.actual_count)){
                if(cursor.inferred.present==0){host.fail(context,Problem::MissingValue,cursor.name,address(),0,0);return Need::Done;}
                cursor.scalar=cursor.inferred.value;cursor.phase=9;continue;
            }
            ParamNode formal=row<ParamNode>(cursor.function.storage,item(cursor.function.storage,declaration.params,cursor.position));
            SignatureNode source=child(cursor.function,formal.ty.offset);
            ActualArgument actual=host.actual(context,cursor.position);cursor.position+=1u64;
            if(actual.count.present!=0&&!infer_const_count(source,cursor.name.bytes,cursor.name.length,actual.count.value,cursor.inferred)){
                host.fail(context,Problem::RepeatCount,cursor.name,address(),0,0);return Need::Done;
            }
            if(address_bits(actual.ty.value)!=0u64){
                cursor.next=source;cursor.next_actual=actual.ty;
                return give_special(state,cursor,Need::ValueChild);
            }
            continue;
        }
        if(cursor.phase==9){
            ValueParameterNode parameter=row<ValueParameterNode>(cursor.function.storage,item(cursor.function.storage,declaration.values,cursor.index));
            if(!accepts_const(cursor.function.storage.nodes,parameter.choices,cursor.scalar)){
                host.fail(context,Problem::ValueDomain,cursor.name,address(),cursor.scalar,0);return Need::Done;
            }
            host.push_value(context,cursor.scalar);cursor.index+=1u64;cursor.phase=6;continue;
        }
        if(cursor.phase==10){return Need::Done;}
    }
}

extern "C" MatchCursor match_start(MatchMode mode,SignatureNode source,u64 function_offset,TypeInfo actual,
    u64 index,NameBytes name,Optional<u32> inferred,u32 known) on cpu {
    return MatchCursor(mode,0,source,function_offset,actual,name,NameBytes(address(),0u64),index,0u64,
        inferred,0,0,known,source,absent_type(),absent_semantic(),0);
}
extern "C" Need match_step(address state,Host host,address context) on cpu
    requires nonzero(host.fail) requires nonzero(host.has_type) requires nonzero(host.same_type)
    requires nonzero(host.keep_type) requires nonzero(host.child) requires nonzero(host.element)
    requires nonzero(host.view) requires nonzero(host.dataset_syntax) requires nonzero(host.cfn_slot)
    requires nonzero(host.record_matches) {
    MatchCursor cursor=unsafe_position<MatchCursor>(state);
    TypeNode source=row<TypeNode>(cursor.source.storage,cursor.source.offset);
    while(true){
        if(cursor.phase==0){
            if(cursor.mode==MatchMode::Type&&cursor.known==0&&!type_mentions(cursor.source,cursor.name.bytes,cursor.name.length)){
                return give_match(state,cursor,Need::Done);
            }
            cursor.phase=1;
            if(source.dataset!=0&&(cursor.actual.flags&1)!=0){
                if(cursor.mode==MatchMode::Value&&cursor.actual.value_count!=0u64){
                    u32 count=unsafe_position<u32>(cursor.actual.values);
                    if(!infer_const_count(cursor.source,cursor.name.bytes,cursor.name.length,count,cursor.inferred)){
                        host.fail(context,Problem::DatasetCount,cursor.name,address(),0,0);return Need::Done;
                    }
                }
                cursor.semantic=host.dataset_syntax(cursor.actual.value);cursor.unit=0;
                return give_match(state,cursor,Need::ActualType);
            }
        }
        if(cursor.phase==1){
            if(source.view!=0){
                if((cursor.actual.flags&2)==0){
                    if(cursor.mode==MatchMode::Value){return give_match(state,cursor,Need::Done);}
                    host.fail(context,Problem::ViewExpected,cursor.name,address(),0,0);return Need::Done;
                }
                cursor.actual=host.view(cursor.actual.value);
            }
            if(cursor.mode==MatchMode::Value&&source.arguments.count==0u64){return give_match(state,cursor,Need::Done);}
            NameBytes source_name=text(cursor.source.storage,source.name);
            if(cursor.mode==MatchMode::Type&&same(source_name,cursor.name)){
                if(source.arguments.count!=0u64){host.fail(context,Problem::TypeParameterArguments,cursor.name,address(),0,0);return Need::Done;}
                FunctionNode function=row<FunctionNode>(cursor.source.storage,cursor.function_offset);
                TypeParameterNode parameter=row<TypeParameterNode>(cursor.source.storage,item(cursor.source.storage,function.types,cursor.index));
                cursor.phase=2;cursor.scalar=1;
                if(parameter.family.present!=0){return give_match(state,cursor,Need::FamilyActual);}
                continue;
            }
            if(word(source_name,"cfn")){
                if((cursor.actual.flags&16)==0){
                    host.fail(context,cursor.mode==MatchMode::Type ? Problem::CfnType : Problem::CfnValue,cursor.name,address(),0,0);return Need::Done;
                }
                if(source.arguments.count!=cursor.actual.parameters+1u64){
                    host.fail(context,cursor.mode==MatchMode::Type ? Problem::CfnArity : Problem::CfnValueArity,cursor.name,address(),0,0);return Need::Done;
                }
                cursor.position=0u64;cursor.phase=3;continue;
            }
            if(word(source_name,"result")){
                if((cursor.actual.flags&8)==0||source.arguments.count!=2u64){
                    host.fail(context,cursor.mode==MatchMode::Type ? Problem::ResultType : Problem::ResultValue,cursor.name,address(),0,0);return Need::Done;
                }
                cursor.position=0u64;cursor.phase=5;continue;
            }
            if(word(source_name,"array")){
                if((cursor.actual.flags&4)==0){
                    host.fail(context,cursor.mode==MatchMode::Type ? Problem::ArrayType : Problem::ArrayValue,cursor.name,address(),0,0);return Need::Done;
                }
                if(cursor.mode==MatchMode::Type&&source.arguments.count==0u64){
                    host.fail(context,Problem::ArrayElement,cursor.name,address(),0,0);return Need::Done;
                }
                if(cursor.mode==MatchMode::Value&&source.arguments.count!=2u64){
                    host.fail(context,Problem::ArrayArity,cursor.name,address(),0,0);return Need::Done;
                }
                ArgumentNode argument=row<ArgumentNode>(cursor.source.storage,item(cursor.source.storage,source.arguments,0u64));
                bool descend=false;
                match(argument){
                    ArgumentNode::Type(value){cursor.next=child(cursor.source,value.offset);descend=true;}
                    ArgumentNode::Value(value){
                        if(cursor.mode==MatchMode::Type){host.fail(context,Problem::ArrayElement,cursor.name,address(),0,0);return Need::Done;}
                    }
                }
                if(cursor.mode==MatchMode::Type&&source.arguments.count!=2u64){
                    host.fail(context,Problem::ArrayArity,cursor.name,address(),0,0);return Need::Done;
                }
                cursor.phase=cursor.mode==MatchMode::Type ? 9 : 6;
                if(descend){
                    cursor.next_actual=host.element(cursor.actual.value);cursor.next_known=0;
                    return give_match(state,cursor,child_need(cursor.mode));
                }
                continue;
            }
            cursor.phase=7;return give_match(state,cursor,Need::RecordName);
        }
        if(cursor.phase==2){
            if(cursor.scalar==0){host.fail(context,Problem::InferredFamily,cursor.name,cursor.actual.value,0,0);return Need::Done;}
            if(host.has_type(context)){
                if(!host.same_type(context,cursor.actual.value)){host.fail(context,Problem::TypeMismatch,cursor.name,cursor.actual.value,0,0);return Need::Done;}
            }else{host.keep_type(context,cursor.actual.value);}
            return give_match(state,cursor,Need::Done);
        }
        if(cursor.phase==3){
            if(cursor.position==source.arguments.count){return give_match(state,cursor,Need::Done);}
            ArgumentNode argument=row<ArgumentNode>(cursor.source.storage,item(cursor.source.storage,source.arguments,cursor.position));
            TypeRef reference;
            match(argument){
                ArgumentNode::Type(value){reference=value;}
                ArgumentNode::Value(value){host.fail(context,Problem::CfnTypeArgument,cursor.name,address(),0,0);return Need::Done;}
            }
            SignatureNode next=child(cursor.source,reference.offset);
            CfnSlot slot=host.cfn_slot(cursor.actual.value,cursor.position);
            if(cursor.position!=0u64){
                CfnParameter projected=cfn_parameter(next);
                if(projected.access!=slot.access){host.fail(context,Problem::CfnAccess,cursor.name,address(),0,0);return Need::Done;}
                if(projected.type.present==0){host.fail(context,Problem::CfnPosition,cursor.name,address(),0,0);return Need::Done;}
                next=child(cursor.source,projected.type.value.offset);
            }
            if(cursor.mode==MatchMode::Type&&!type_mentions(next,cursor.name.bytes,cursor.name.length)){
                cursor.position+=1u64;continue;
            }
            if(cursor.mode==MatchMode::Value&&(plain_void(slot.ty)||slot.ty.never!=0)){
                cursor.position+=1u64;continue;
            }
            cursor.next=next;cursor.semantic=slot.ty;
            cursor.unit=cursor.mode==MatchMode::Type&&plain_void(slot.ty) ? 1 : 0;
            cursor.next_known=cursor.mode==MatchMode::Type ? 1 : 0;cursor.phase=4;
            return give_match(state,cursor,Need::ChildType);
        }
        if(cursor.phase==4){
            cursor.position+=1u64;cursor.phase=3;
            return give_match(state,cursor,child_need(cursor.mode));
        }
        if(cursor.phase==5){
            if(cursor.position==smaller(source.arguments.count,cursor.actual.arguments)){return give_match(state,cursor,Need::Done);}
            ArgumentNode argument=row<ArgumentNode>(cursor.source.storage,item(cursor.source.storage,source.arguments,cursor.position));
            TypeRef reference;
            match(argument){
                ArgumentNode::Type(value){reference=value;}
                ArgumentNode::Value(value){host.fail(context,Problem::ResultTypeArgument,cursor.name,address(),0,0);return Need::Done;}
            }
            cursor.next=child(cursor.source,reference.offset);
            cursor.next_actual=host.child(cursor.actual.value,cursor.position);cursor.next_known=0;cursor.position+=1u64;
            return give_match(state,cursor,child_need(cursor.mode));
        }
        if(cursor.phase==6){
            Optional<u32> conflict=infer_const_arguments(cursor.source,cursor.name.bytes,cursor.name.length,1u64,
                cursor.actual.values,cursor.actual.value_count,cursor.inferred);
            if(conflict.present!=0){host.fail(context,Problem::ArrayCount,cursor.name,address(),0,0);return Need::Done;}
            return give_match(state,cursor,Need::Done);
        }
        if(cursor.phase==7){
            if(source.arguments.count!=cursor.actual.arguments+cursor.actual.value_count||!host.record_matches(cursor.actual.value,cursor.base)){
                if(cursor.mode==MatchMode::Type){
                    host.fail(context,Problem::RecordType,text(cursor.source.storage,source.name),cursor.actual.value,0,0);
                }else{host.fail(context,Problem::RecordValue,cursor.name,address(),0,0);}
                return Need::Done;
            }
            cursor.position=0u64;cursor.phase=8;
        }
        if(cursor.phase==8){
            if(cursor.position==smaller(source.arguments.count,cursor.actual.arguments)){
                if(cursor.mode==MatchMode::Value){
                    Optional<u32> conflict=infer_const_arguments(cursor.source,cursor.name.bytes,cursor.name.length,
                        cursor.actual.arguments,cursor.actual.values,cursor.actual.value_count,cursor.inferred);
                    if(conflict.present!=0){host.fail(context,Problem::RecordConstConflict,cursor.name,address(),cursor.inferred.value,conflict.value);return Need::Done;}
                }
                return give_match(state,cursor,Need::Done);
            }
            ArgumentNode argument=row<ArgumentNode>(cursor.source.storage,item(cursor.source.storage,source.arguments,cursor.position));
            bool descend=false;
            match(argument){
                ArgumentNode::Type(value){cursor.next=child(cursor.source,value.offset);descend=true;}
                ArgumentNode::Value(value){
                    if(cursor.mode==MatchMode::Type){host.fail(context,Problem::RecordTypeArgument,cursor.name,address(),0,0);return Need::Done;}
                }
            }
            u64 position=cursor.position;cursor.position+=1u64;
            if(descend){
                cursor.next_actual=host.child(cursor.actual.value,position);cursor.next_known=0;
                return give_match(state,cursor,child_need(cursor.mode));
            }
            continue;
        }
        if(cursor.phase==9){return give_match(state,cursor,Need::Done);}
    }
}

extern "C" InstanceCursor instance_start(SignatureNode function,u32 tracking,u64 type_count,u64 value_count) on cpu {
    return InstanceCursor(function,0,tracking,type_count,value_count,0u64);
}
extern "C" Need instance_step(address state,InstanceHost host,address context) on cpu
    requires nonzero(host.implementation) requires nonzero(host.external) requires nonzero(host.plain)
    requires nonzero(host.name) requires nonzero(host.type_argument) requires nonzero(host.value_argument)
    requires nonzero(host.constant) requires nonzero(host.nonzero_error) requires nonzero(host.uses)
    requires nonzero(host.construct) {
    InstanceCursor cursor=unsafe_position<InstanceCursor>(state);
    FunctionNode declaration=row<FunctionNode>(cursor.function.storage,cursor.function.offset);
    while(true){
        if(cursor.phase==0){
            if(cursor.tracking!=0){
                if(declaration.kind!=2&&declaration.body.present!=0){host.implementation(context);}
                if(declaration.external.present!=0){host.external(context);}
            }
            if(declaration.types.count==0u64&&declaration.values.count==0u64){host.plain(context);return Need::Done;}
            host.name(context);
            cursor.index=0u64;
            while(cursor.index<smaller(declaration.types.count,cursor.type_count)){
                TypeParameterNode parameter=row<TypeParameterNode>(cursor.function.storage,item(cursor.function.storage,declaration.types,cursor.index));
                host.type_argument(context,text(cursor.function.storage,parameter.name));cursor.index+=1u64;
            }
            cursor.index=0u64;
            while(cursor.index<smaller(declaration.values.count,cursor.value_count)){
                ValueParameterNode parameter=row<ValueParameterNode>(cursor.function.storage,item(cursor.function.storage,declaration.values,cursor.index));
                host.value_argument(context,text(cursor.function.storage,parameter.name),cursor.index);cursor.index+=1u64;
            }
            cursor.index=0u64;
            while(cursor.index<declaration.contracts.count){
                ContractNode contract=row<ContractNode>(cursor.function.storage,item(cursor.function.storage,declaration.contracts,cursor.index));
                if(contract.kind==0){
                    PredicateNode predicate=row<PredicateNode>(cursor.function.storage,contract.predicate.offset);
                    match(predicate){
                        PredicateNode::Nonzero(value){
                            NameBytes name=text(cursor.function.storage,value);Optional<u32> bound=host.constant(context,name);
                            if(bound.present!=0&&bound.value==0){host.nonzero_error(context,cursor.index,name);return Need::Done;}
                        }
                        PredicateNode::Prefix(value){}
                        PredicateNode::Nonempty(value){}
                        PredicateNode::Index(value){}
                        PredicateNode::Contiguous(value){}
                        PredicateNode::Initialized(value){}
                        PredicateNode::SameLength(value){}
                        PredicateNode::Cleanup(value){}
                        PredicateNode::Memory(value){}
                        PredicateNode::Range(value){}
                    }
                }
                cursor.index+=1u64;
            }
            if(!host.uses(context)){return Need::Done;}
            host.construct(context);cursor.phase=1;
            if(declaration.c_abi!=0){return give_instance(state,cursor,Need::CAbi);}
        }
        if(cursor.phase==1){cursor.phase=2;return give_instance(state,cursor,Need::Interface);}
        if(cursor.phase==2){return Need::Done;}
    }
}

extern "C" bool is_open(StorageView type_storage,Slice types,StorageView value_storage,Slice values) on cpu {
    u64 index=0u64;
    while(index<types.count){
        TypeParameterNode parameter=row<TypeParameterNode>(type_storage,item(type_storage,types,index));
        if(parameter.family.present==0){return true;}
        index+=1u64;
    }
    index=0u64;
    while(index<values.count){
        ValueParameterNode parameter=row<ValueParameterNode>(value_storage,item(value_storage,values,index));
        if(parameter.choices.present==0){return true;}
        index+=1u64;
    }
    return false;
}
