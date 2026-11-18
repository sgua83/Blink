module bootstrap.stack_report;

address metric_at(address data,u64 index) on cpu {
    return unsafe_address(address_bits(data)+index*size_of<StackMetric>());
}
address scratch_at(address data,u64 index) on cpu {
    return unsafe_address(address_bits(data)+index*size_of<StackScratch>());
}
StackAmount add_amount(StackAmount value,u64 add) on cpu {
    u64 low=value.low+add;
    return StackAmount(low,value.high+(low<value.low?1u64:0u64));
}
bool less_amount(StackAmount left,StackAmount right) on cpu {
    return left.high<right.high||(left.high==right.high&&left.low<right.low);
}
void raise_amount(address row,StackAmount value) on cpu {
    if(less_amount(unsafe_position<StackMetric>(row).amount,value)){
        unsafe_position<StackMetric>(row).amount=value;
    }
}

extern "C" void analyze(address context,u64 frames,u64 symbols,u32 has_budget,u64 budget,
    address metrics,address scratch,StackHost host) on cpu
    requires nonzero(host.frame) requires nonzero(host.call)
    requires nonzero(host.register_frame) requires nonzero(host.register_symbol)
    requires nonzero(host.symbol) requires nonzero(host.position) requires nonzero(host.edge)
    requires nonzero(host.parent_count) requires nonzero(host.parent) requires nonzero(host.notice)
    requires nonzero(host.union_unknown) requires nonzero(host.unknown_empty) {
    u64 index=0u64;
    while(index<frames){
        StackFrame frame=host.frame(context,index);
        if(frame.positioned!=0){host.register_frame(context,index);}
        unsafe_position<StackMetric>(metric_at(metrics,index))=StackMetric(StackAmount(u64(frame.local)+8u64,0u64),0,0,StackBudget::NotSupplied);
        unsafe_position<StackScratch>(scratch_at(scratch,index))=StackScratch(0u64,0u64);
        index+=1u64;
    }
    index=0u64;
    while(index<symbols){host.register_symbol(context,index);index+=1u64;}
    index=0u64;
    while(index<frames){
        StackFrame frame=host.frame(context,index);u64 at=0u64;
        while(at<frame.calls){
            StackCall call=host.call(context,index,at);at+=1u64;
            if(call.kind==StackCallKind::Unreported||call.kind==StackCallKind::Indirect){
                if(call.kind==StackCallKind::Unreported){host.notice(context,index,StackNotice::Unreported,call);}
                else{host.notice(context,index,StackNotice::Indirect,call);}
                raise_amount(metric_at(metrics,index),StackAmount(u64(frame.local)+16u64,0u64));
                continue;
            }
            StackPosition position=StackPosition(call.section,call.start,1);
            if(call.target==StackTarget::Symbol){
                position=host.symbol(context,call.name);
                if(position.present==0){host.notice(context,index,StackNotice::UnresolvedModule,call);}
            }else if(call.target==StackTarget::Import){
                position.present=0;host.notice(context,index,StackNotice::External,call);
            }
            StackIndex child=StackIndex(0u64,0);
            if(position.present!=0){
                child=host.position(context,position);
                if(child.present!=0){
                    host.edge(context,index,child.value,call.kind==StackCallKind::Tail?1:0);
                    unsafe_position<StackScratch>(scratch_at(scratch,index)).pending+=1u64;
                }else{
                    call.section=position.section;call.start=position.start;
                    host.notice(context,index,StackNotice::MissingBody,call);
                }
            }
            if(child.present==0){
                u64 outgoing=call.kind==StackCallKind::Tail?8u64:16u64;
                raise_amount(metric_at(metrics,index),StackAmount(u64(frame.local)+outgoing,0u64));
            }
        }
        index+=1u64;
    }
    u64 head=0u64;u64 tail=0u64;index=0u64;
    while(index<frames){
        if(unsafe_position<StackScratch>(scratch_at(scratch,index)).pending==0u64){
            unsafe_position<StackScratch>(scratch_at(scratch,tail)).ready=index;tail+=1u64;
        }
        index+=1u64;
    }
    while(head<tail){
        u64 child=unsafe_position<StackScratch>(scratch_at(scratch,head)).ready;head+=1u64;
        unsafe_position<StackMetric>(metric_at(metrics,child)).closed=1;
        u64 count=host.parent_count(context,child);u64 at=0u64;
        while(at<count){
            StackParent parent=host.parent(context,child,at);at+=1u64;
            StackFrame frame=host.frame(context,parent.parent);
            u64 local=u64(frame.local)+(parent.tail!=0?0u64:8u64);
            StackAmount amount=add_amount(unsafe_position<StackMetric>(metric_at(metrics,child)).amount,local);
            raise_amount(metric_at(metrics,parent.parent),amount);
            host.union_unknown(context,parent.parent,child);
            address pending=scratch_at(scratch,parent.parent);
            unsafe_position<StackScratch>(pending).pending-=1u64;
            if(unsafe_position<StackScratch>(pending).pending==0u64){
                unsafe_position<StackScratch>(scratch_at(scratch,tail)).ready=parent.parent;tail+=1u64;
            }
        }
    }
    index=0u64;
    while(index<frames){
        address row=metric_at(metrics,index);StackMetric metric=unsafe_position<StackMetric>(row);
        metric.upper=metric.closed!=0&&host.unknown_empty(context,index)!=0?1:0;
        if(has_budget!=0){
            StackFrame frame=host.frame(context,index);
            if(u64(frame.local)+8u64>budget){metric.capacity=StackBudget::LocalExceeds;}
            else if(metric.upper!=0){
                metric.capacity=metric.amount.high==0u64&&metric.amount.low<=budget?StackBudget::UpperFits:StackBudget::UpperExceeds;
            }else{metric.capacity=StackBudget::Unknown;}
        }
        unsafe_position<StackMetric>(row)=metric;index+=1u64;
    }
}
