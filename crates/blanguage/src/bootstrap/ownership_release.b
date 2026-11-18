module bootstrap.ownership_release;
extern "C" Cursor start() on cpu {return Cursor(0,0u64);}
extern "C" Entry entry(u32 may_own,u32 pending,u32 stored_words) on cpu {
    if(may_own!=0&&pending!=0&&stored_words!=0){return Entry::ErrorPosition;}
    return Entry::Present;
}
extern "C" Step<Need> step(Cursor state,Facts facts) on cpu {
    Cursor cursor=state;
    if(cursor.phase==0){
        if(facts.ownership==State::Conditional){return Step<Need>(cursor,Need::Conditional);}
        if(facts.ownership!=State::Owned){return Step<Need>(cursor,Need::Complete);}
        if(facts.kind==Kind::Data){cursor.phase=1;return Step<Need>(cursor,Need::Frame);}
        if(facts.kind!=Kind::Owned){return Step<Need>(cursor,Need::Complete);}
        cursor.phase=2;return Step<Need>(cursor,Need::Inspect);
    }
    if(cursor.phase==1){return Step<Need>(cursor,Need::Data);}
    if(cursor.phase==2){
        cursor.phase=3;
        if(facts.presence!=0&&facts.placed!=0){return Step<Need>(cursor,Need::Load);}
        return Step<Need>(cursor,Need::Next);
    }
    if(cursor.phase==3){
        if(facts.task!=0){return Step<Need>(cursor,Need::Task);}
        if(facts.cleanup!=0){return Step<Need>(cursor,Need::User);}
        if(facts.variants!=0){cursor.phase=20;return Step<Need>(cursor,Need::ClearPlace);}
        cursor.index=facts.fields;cursor.phase=10;return Step<Need>(cursor,Need::Next);
    }
    if(cursor.phase==10){
        if(cursor.index==0u64){return Step<Need>(cursor,Need::Complete);}
        cursor.index-=1u64;cursor.phase=11;return Step<Need>(cursor,Need::InspectField);
    }
    if(cursor.phase==11){
        if(facts.field_owns==0){cursor.phase=10;return Step<Need>(cursor,Need::Next);}
        cursor.phase=12;return Step<Need>(cursor,Need::Unpack);
    }
    if(cursor.phase==12){cursor.phase=13;return Step<Need>(cursor,Need::Forget);}
    if(cursor.phase==13){cursor.phase=14;return Step<Need>(cursor,Need::Borrow);}
    if(cursor.phase==14){cursor.phase=10;return Step<Need>(cursor,Need::Recurse);}
    return Step<Need>(cursor,Need::Variant);
}
extern "C" Step<VariantNeed> variant_step(Cursor state,VariantFacts facts) on cpu {
    Cursor cursor=state;
    if(cursor.phase==0){
        if(cursor.index+1u64==facts.count){cursor.phase=20;return Step<VariantNeed>(cursor,VariantNeed::Unpack);}
        cursor.phase=1;return Step<VariantNeed>(cursor,VariantNeed::Condition);
    }
    if(cursor.phase==1){
        if(facts.known==2){cursor.phase=20;return Step<VariantNeed>(cursor,VariantNeed::Unpack);}
        if(facts.known==1){cursor.phase=30;return Step<VariantNeed>(cursor,VariantNeed::Rest);}
        cursor.phase=2;return Step<VariantNeed>(cursor,VariantNeed::Outer);
    }
    if(cursor.phase==2){cursor.phase=3;return Step<VariantNeed>(cursor,VariantNeed::Unpack);}
    if(cursor.phase==3){cursor.phase=4;return Step<VariantNeed>(cursor,VariantNeed::Drop);}
    if(cursor.phase==4){cursor.phase=5;return Step<VariantNeed>(cursor,VariantNeed::Yes);}
    if(cursor.phase==5){cursor.phase=6;return Step<VariantNeed>(cursor,VariantNeed::Rest);}
    if(cursor.phase==6){cursor.phase=30;return Step<VariantNeed>(cursor,VariantNeed::Emit);}
    if(cursor.phase==20){cursor.phase=30;return Step<VariantNeed>(cursor,VariantNeed::Drop);}
    return Step<VariantNeed>(cursor,VariantNeed::Complete);
}
extern "C" Step<UserNeed> user_step(Cursor state,Kind payload) on cpu {
    Cursor cursor=state;
    if(cursor.phase==0){cursor.phase=1;return Step<UserNeed>(cursor,UserNeed::Payload);}
    if(cursor.phase==1){
        cursor.phase=2;
        if(payload==Kind::Data){return Step<UserNeed>(cursor,UserNeed::MovableData);}
        if(payload==Kind::Owned){return Step<UserNeed>(cursor,UserNeed::MovableTree);}
        return Step<UserNeed>(cursor,UserNeed::Next);
    }
    if(cursor.phase==2){cursor.phase=3;return Step<UserNeed>(cursor,UserNeed::Argument);}
    if(cursor.phase==3){cursor.phase=4;return Step<UserNeed>(cursor,UserNeed::Instance);}
    if(cursor.phase==4){cursor.phase=5;return Step<UserNeed>(cursor,UserNeed::Invoke);}
    return Step<UserNeed>(cursor,UserNeed::Complete);
}
extern "C" Step<u32> resource_step(Cursor state,u64 count) on cpu {
    Cursor cursor=state;
    if(cursor.index>=count){return Step<u32>(cursor,0);}
    cursor.index+=1u64;return Step<u32>(cursor,1);
}
