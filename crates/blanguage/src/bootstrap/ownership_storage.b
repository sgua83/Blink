module bootstrap.ownership_storage;

extern "C" Cursor start() on cpu {return Cursor(0,0u64);}
extern "C" Layout dataset_layout(u32 root_size,u32 root_align,u32 length_size,u32 length_align) on cpu {
    u32 alignment=root_align>length_align ? root_align : length_align;
    return Layout(root_size,alignment,(root_size+length_size+alignment-1)&(0-alignment));
}
extern "C" Step<StorageNeed> storage_step(Cursor state,Kind kind) on cpu {
    Cursor cursor=state;
    if(cursor.phase==0){
        if(kind==Kind::Data){cursor.phase=1;return Step<StorageNeed>(cursor,StorageNeed::RootProof);}
        cursor.phase=9;
        if(kind==Kind::Value){return Step<StorageNeed>(cursor,StorageNeed::Value);}
        if(kind==Kind::Owned){return Step<StorageNeed>(cursor,StorageNeed::Owned);}
        return Step<StorageNeed>(cursor,StorageNeed::Unit);
    }
    if(cursor.phase==1){cursor.phase=2;return Step<StorageNeed>(cursor,StorageNeed::LengthProof);}
    if(cursor.phase==2){cursor.phase=9;return Step<StorageNeed>(cursor,StorageNeed::Data);}
    return Step<StorageNeed>(cursor,StorageNeed::Complete);
}
extern "C" Step<RebindNeed> rebind_step(Cursor state,Kind kind) on cpu {
    Cursor cursor=state;
    if(cursor.phase==0){
        if(kind==Kind::Unit){return Step<RebindNeed>(cursor,RebindNeed::Complete);}
        if(kind==Kind::Data){cursor.phase=2;return Step<RebindNeed>(cursor,RebindNeed::Root);}
        if(kind==Kind::Owned){cursor.phase=1;return Step<RebindNeed>(cursor,RebindNeed::Owned);}
        cursor.phase=9;return Step<RebindNeed>(cursor,RebindNeed::Value);
    }
    if(cursor.phase==1){cursor.phase=9;return Step<RebindNeed>(cursor,RebindNeed::OwnedPlace);}
    if(cursor.phase==2){cursor.phase=9;return Step<RebindNeed>(cursor,RebindNeed::Length);}
    return Step<RebindNeed>(cursor,RebindNeed::Complete);
}
extern "C" Step<UnpackNeed> unpack_step(Cursor state,UnpackFacts facts) on cpu {
    Cursor cursor=state;
    if(cursor.phase==0){if(facts.owns==0){return Step<UnpackNeed>(cursor,UnpackNeed::Value);}cursor.phase=1;return Step<UnpackNeed>(cursor,UnpackNeed::Clone);}
    if(cursor.phase==1){cursor.phase=2;return Step<UnpackNeed>(cursor,UnpackNeed::Inspect);}
    if(cursor.phase==2){
        if(facts.kind==Kind::Unit){return Step<UnpackNeed>(cursor,UnpackNeed::Complete);}
        if(facts.kind==Kind::Value){cursor.phase=9;return Step<UnpackNeed>(cursor,UnpackNeed::ValueRegs);}
        if(facts.kind==Kind::Owned){cursor.phase=8;return Step<UnpackNeed>(cursor,UnpackNeed::Owned);}
        cursor.phase=3;return Step<UnpackNeed>(cursor,UnpackNeed::Root);
    }
    if(cursor.phase==3){cursor.phase=4;return Step<UnpackNeed>(cursor,UnpackNeed::Length);}
    if(cursor.phase==4){cursor.phase=8;if(facts.count!=0){return Step<UnpackNeed>(cursor,UnpackNeed::Count);}return Step<UnpackNeed>(cursor,UnpackNeed::Next);}
    if(cursor.phase==8){cursor.phase=9;if(facts.place!=0){return Step<UnpackNeed>(cursor,UnpackNeed::Place);}return Step<UnpackNeed>(cursor,UnpackNeed::Next);}
    return Step<UnpackNeed>(cursor,UnpackNeed::Complete);
}
extern "C" Step<TemplateNeed> template_step(Cursor state,TemplateFacts facts) on cpu {
    Cursor cursor=state;
    if(cursor.phase==0){cursor.phase=1;if(facts.dataset==0){return Step<TemplateNeed>(cursor,TemplateNeed::Retain);}return Step<TemplateNeed>(cursor,TemplateNeed::Next);}
    if(cursor.phase==1){
        if(facts.dataset!=0){cursor.phase=2;return Step<TemplateNeed>(cursor,TemplateNeed::Element);}
        if(facts.owns!=0){
            if(facts.variant!=0){cursor.phase=10;return Step<TemplateNeed>(cursor,TemplateNeed::VariantSlots);}
            cursor.phase=20;return Step<TemplateNeed>(cursor,TemplateNeed::Next);
        }
        if(facts.void_type!=0){return Step<TemplateNeed>(cursor,TemplateNeed::Unit);}return Step<TemplateNeed>(cursor,TemplateNeed::Value);
    }
    if(cursor.phase==2){cursor.phase=3;return Step<TemplateNeed>(cursor,TemplateNeed::Handle);}
    if(cursor.phase==3){cursor.phase=4;return Step<TemplateNeed>(cursor,TemplateNeed::Length);}
    if(cursor.phase==4){cursor.phase=5;if(facts.count!=0){return Step<TemplateNeed>(cursor,TemplateNeed::FixedLength);}return Step<TemplateNeed>(cursor,TemplateNeed::LengthProof);}
    if(cursor.phase==5){if(facts.available!=0){return Step<TemplateNeed>(cursor,TemplateNeed::DataOwned);}return Step<TemplateNeed>(cursor,TemplateNeed::DataMoved);}
    if(cursor.phase==10){if(facts.available==0){return Step<TemplateNeed>(cursor,TemplateNeed::VariantMoved);}cursor.index=0u64;cursor.phase=11;return Step<TemplateNeed>(cursor,TemplateNeed::Next);}
    if(cursor.phase==11){if(cursor.index>=facts.variants){return Step<TemplateNeed>(cursor,TemplateNeed::VariantOwned);}cursor.phase=12;return Step<TemplateNeed>(cursor,TemplateNeed::VariantField);}
    if(cursor.phase==12){cursor.phase=13;return Step<TemplateNeed>(cursor,TemplateNeed::Recurse);}
    if(cursor.phase==13){cursor.phase=14;return Step<TemplateNeed>(cursor,TemplateNeed::VariantStore);}
    if(cursor.phase==14){cursor.index+=1u64;cursor.phase=11;return Step<TemplateNeed>(cursor,TemplateNeed::Next);}
    if(cursor.phase==20){
        if(cursor.index>=facts.fields){if((facts.cleanup!=0||facts.task!=0)&&facts.available==0){return Step<TemplateNeed>(cursor,TemplateNeed::AggregateMoved);}return Step<TemplateNeed>(cursor,TemplateNeed::AggregateOwned);}
        cursor.phase=21;return Step<TemplateNeed>(cursor,TemplateNeed::InspectField);
    }
    if(cursor.phase==21){if(facts.child_owns!=0){cursor.phase=22;return Step<TemplateNeed>(cursor,TemplateNeed::Field);}cursor.phase=24;return Step<TemplateNeed>(cursor,TemplateNeed::AggregateEmpty);}
    if(cursor.phase==22){cursor.phase=23;return Step<TemplateNeed>(cursor,TemplateNeed::Recurse);}
    if(cursor.phase==23){cursor.phase=24;return Step<TemplateNeed>(cursor,TemplateNeed::AggregateStore);}
    cursor.index+=1u64;cursor.phase=20;return Step<TemplateNeed>(cursor,TemplateNeed::Next);
}
extern "C" Step<ConstructNeed> construct_step(Cursor state,ConstructFacts facts) on cpu {
    Cursor cursor=state;
    if(cursor.phase==0){
        if(facts.variant!=0){return Step<ConstructNeed>(cursor,ConstructNeed::Variant);}
        if(facts.cpu==0||facts.application==0){return Step<ConstructNeed>(cursor,ConstructNeed::Target);}
        if(facts.arguments!=facts.fields&&!(facts.array!=0&&facts.arguments==1u64)){return Step<ConstructNeed>(cursor,ConstructNeed::Count);}
        cursor.phase=1;return Step<ConstructNeed>(cursor,ConstructNeed::Next);
    }
    if(cursor.phase==1){if(cursor.index>=facts.arguments){cursor.phase=20;return Step<ConstructNeed>(cursor,ConstructNeed::Finish);}cursor.phase=2;return Step<ConstructNeed>(cursor,ConstructNeed::Evaluate);}
    if(cursor.phase==2){
        if(facts.continues==0){cursor.index=0u64;cursor.phase=90;return Step<ConstructNeed>(cursor,ConstructNeed::Next);}
        if(facts.present==0){return Step<ConstructNeed>(cursor,ConstructNeed::Missing);}
        if(facts.array!=0&&facts.arguments==1u64){cursor.phase=3;return Step<ConstructNeed>(cursor,ConstructNeed::InspectArray);}
        cursor.phase=4;return Step<ConstructNeed>(cursor,ConstructNeed::Next);
    }
    if(cursor.phase==3){if(facts.same_array!=0){return Step<ConstructNeed>(cursor,ConstructNeed::ReturnItem);}if(facts.fields!=1u64){return Step<ConstructNeed>(cursor,ConstructNeed::Copy);}cursor.phase=4;return Step<ConstructNeed>(cursor,ConstructNeed::Next);}
    if(cursor.phase==4){cursor.phase=5;return Step<ConstructNeed>(cursor,ConstructNeed::Check);}
    if(cursor.phase==5){cursor.phase=6;return Step<ConstructNeed>(cursor,ConstructNeed::InspectHold);}
    if(cursor.phase==6){
        cursor.phase=7;
        if(facts.kind==Kind::Data||facts.kind==Kind::Owned||(facts.kind==Kind::Value&&facts.borrows!=0)){return Step<ConstructNeed>(cursor,ConstructNeed::Hold);}
        return Step<ConstructNeed>(cursor,ConstructNeed::Store);
    }
    if(cursor.phase==7){cursor.index+=1u64;cursor.phase=1;return Step<ConstructNeed>(cursor,ConstructNeed::Next);}
    if(cursor.phase==20){cursor.index=0u64;cursor.phase=21;return Step<ConstructNeed>(cursor,ConstructNeed::Next);}
    if(cursor.phase==21){if(cursor.index>=facts.arguments){return Step<ConstructNeed>(cursor,ConstructNeed::Pack);}cursor.phase=22;return Step<ConstructNeed>(cursor,ConstructNeed::Take);}
    if(cursor.phase==22){cursor.phase=23;if(facts.held!=0){return Step<ConstructNeed>(cursor,ConstructNeed::Unhold);}return Step<ConstructNeed>(cursor,ConstructNeed::Next);}
    if(cursor.phase==23){cursor.phase=24;return Step<ConstructNeed>(cursor,ConstructNeed::InspectItem);}
    if(cursor.phase==24){cursor.phase=25;if(facts.kind==Kind::Value&&facts.field_owns==0){return Step<ConstructNeed>(cursor,ConstructNeed::MoveValue);}return Step<ConstructNeed>(cursor,ConstructNeed::Storage);}
    if(cursor.phase==25){cursor.phase=26;if(facts.field_owns!=0){return Step<ConstructNeed>(cursor,ConstructNeed::Payload);}return Step<ConstructNeed>(cursor,ConstructNeed::EmptyPayload);}
    if(cursor.phase==26){cursor.index+=1u64;cursor.phase=21;return Step<ConstructNeed>(cursor,ConstructNeed::Next);}
    if(cursor.phase==90){if(cursor.index>=facts.prepared){return Step<ConstructNeed>(cursor,ConstructNeed::Empty);}cursor.phase=91;return Step<ConstructNeed>(cursor,ConstructNeed::AbandonInspect);}
    if(cursor.phase==91){cursor.phase=92;if(facts.held!=0){return Step<ConstructNeed>(cursor,ConstructNeed::AbandonRemove);}return Step<ConstructNeed>(cursor,ConstructNeed::Next);}
    cursor.index+=1u64;cursor.phase=90;return Step<ConstructNeed>(cursor,ConstructNeed::Next);
}
extern "C" void variant_projection(address root,address child,u32 offset,address output,VariantHost host) on cpu
    requires nonzero(host.count) requires nonzero(host.component) requires nonzero(host.select)
    requires nonzero(host.proof) requires nonzero(host.selected) requires nonzero(host.forget) {
    u64 count=host.count(child);u64 i=0u64;
    while(i<count){
        Component wanted=host.component(child,i);u32 word=offset+wanted.word;u64 source=0u64;
        Component actual=host.component(root,source);
        while(!(actual.word<=word&&word<actual.word+actual.size)){source+=1u64;actual=host.component(root,source);}
        host.select(output,source);i+=1u64;
    }
    host.proof(output);i=0u64;
    while(i<count){
        Component wanted=host.component(child,i);Component actual=host.component(root,host.selected(output,i));
        if(wanted.scalar!=actual.scalar||wanted.word!=actual.word-offset){host.forget(output,i,count);}
        i+=1u64;
    }
}
