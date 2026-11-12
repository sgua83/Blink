module bootstrap.parser;
import bootstrap.lexer;
import blink.unicode_scalar;

struct AngleEnd {u64 end;u32 present;};
struct LinkStringResult {u64 count;u32 error;};
struct ParsedParameters {Slice types;Slice values;};

u32 byte_at(address source,u64 at) on cpu {
    return u32(unsafe_position<u8>(unsafe_address(address_bits(source)+at)));
}
TokenSpan span_at(address tokens,u64 at) on cpu {
    return unsafe_position<TokenSpan>(unsafe_address(address_bits(tokens)+at*16u64));
}
bool accepted(u64 length,u32 first,u32 second) on cpu {
    return first>=65&&first<=90 || first>=97&&first<=122 || first>=48&&first<=57 || first==95 || first==34
        || length==1u64 && (first==44||first==46||first==43||first==45||first==42||first==47
            ||first==37||first==38||first==124||first==94||first==126||first==33||first==63||first==58)
        || length==2u64 && (first==58&&second==58 || first==60&&second==60
            ||first==61&&second==61 ||first==33&&second==61 ||first==60&&second==61
            ||first==62&&second==61 ||first==38&&second==38 ||first==124&&second==124);
}
extern "C" AngleEnd angle_end(address source,address tokens,u64 count,u64 at) on cpu {
    if(at>=count){return AngleEnd(0u64,0);}
    TokenSpan start=span_at(tokens,at);
    if(start.end-start.start!=1u64||byte_at(source,start.start)!=60){return AngleEnd(0u64,0);}
    i64 depth=i64(0);u64 parentheses=0u64;u64 brackets=0u64;u64 index=at;
    while(index<count){
        TokenSpan token=span_at(tokens,index);u64 length=token.end-token.start;
        u32 first=0;u32 second=0;
        if(length>0u64){first=byte_at(source,token.start);}
        if(length>1u64){second=byte_at(source,token.start+1u64);}
        if(parentheses!=0u64||brackets!=0u64){
            if(length==1u64){
                if(first==40){parentheses+=1u64;}
                else if(first==41){if(parentheses==0u64){return AngleEnd(0u64,0);}parentheses-=1u64;}
                else if(first==91){brackets+=1u64;}
                else if(first==93){if(brackets==0u64){return AngleEnd(0u64,0);}brackets-=1u64;}
            }
            index+=1u64;continue;
        }
        if(length==1u64&&first==40){parentheses=1u64;index+=1u64;continue;}
        if(length==1u64&&first==91){brackets=1u64;index+=1u64;continue;}
        if(length==1u64&&first==60){depth+=i64(1);}
        else if(length==1u64&&first==62){depth-=i64(1);}
        else if(length==2u64&&first==62&&second==62){depth-=i64(2);}
        else if(!accepted(length,first,second)){return AngleEnd(0u64,0);}
        if(depth==i64(0)){return AngleEnd(index+1u64,1);}
        if(depth<i64(0)){return AngleEnd(0u64,0);}
        index+=1u64;
    }
    return AngleEnd(0u64,0);
}

extern "C" LinkStringResult decode_link_string(address source,u64 length,address output) on cpu {
    u64 at=0u64;u64 count=0u64;
    while(at<length){
        u32 c=byte_at(source,at);at+=1u64;
        if(c==92){
            if(at==length){return LinkStringResult(0u64,1);}
            c=byte_at(source,at);at+=1u64;
            if(c!=92&&c!=34){return LinkStringResult(0u64,1);}
        }
        if(c==0||c>127){return LinkStringResult(0u64,2);}
        unsafe_position<u8>(unsafe_address(address_bits(output)+count))=u8(c);
        count+=1u64;
    }
    if(count==0u64){return LinkStringResult(0u64,3);}
    return LinkStringResult(count,0);
}

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap,u32 flags,usize bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap,u32 flags,address data) on cpu from "KERNEL32.dll";
extern "C" u32 GetLastError() on cpu ensures memory(read) from "KERNEL32.dll";

Text empty_text() on cpu {return Text(0,Span(0u64,0u64));}
Span parse_span(read ParseInputs input,u64 at) on cpu {
    TokenSpan token=span_at(input.tokens,at);
    return Span(token.start,token.end);
}
void parse_storage_error(inout ParseError failure,StorageError error) on cpu {
    if(error.kind!=0){
        if(failure.storage.kind==0){failure.storage=error;}
        if(failure.kind==0){failure.kind=386;failure.span=Span(0u64,0u64);}
    }
}
bool token_is(read ParseInputs input,u64 at,u32 first,u32 second) on cpu {
    if(at>=input.count){return false;}
    TokenSpan token=span_at(input.tokens,at);
    if(second==0){return token.end-token.start==1u64&&byte_at(input.source,token.start)==first;}
    return token.end-token.start==2u64&&byte_at(input.source,token.start)==first&&byte_at(input.source,token.start+1u64)==second;
}
bool identifier_at(read ParseInputs input,u64 at) on cpu {
    if(at>=input.count){return false;}
    TokenSpan token=span_at(input.tokens,at);
    if(token.start==token.end){return false;}
    u32 first=byte_at(input.source,token.start);
    return first>=65&&first<=90||first>=97&&first<=122||first==95;
}
Text parse_identifier(read ParseInputs input,inout ParseState state) on cpu {
    Span span=parse_span(input,state.at);
    if(!identifier_at(input,state.at)){
        state.error.kind=11;state.error.span=span;return empty_text();
    }
    state.at+=1u64;return Text(0,span);
}
// Separators are consumed by the original grammar before copying names. A
// single identifier remains a Source view; a qualified name needs one final
// normalized string. Its temporary allocation ends after storage.text copies.
// Scan the original grammar once. Only the completed normalized path has a
// text consumer; prefixes never enter the persistent syntax text arena.
void scan_qualified(read ParseInputs input,inout ParseState state,u32 separator,bool type_name) on cpu {
    parse_identifier(input,state);
    while(state.error.kind==0&&(separator!=58&&token_is(input,state.at,separator,0)||separator==58&&token_is(input,state.at,58,58))){
        state.at+=1u64;parse_identifier(input,state);
    }
    if(type_name){
        while(state.error.kind==0&&token_is(input,state.at,58,58)){
            state.at+=1u64;parse_identifier(input,state);
        }
    }
}
Text scanned_text(read ParseInputs input,inout ParseState state,u64 start) on cpu {
    if(state.error.kind!=0){return empty_text();}
    if(state.at==start+1u64){return Text(0,parse_span(input,start));}
    u64 bytes=0u64;u64 cursor=start;
    while(cursor<state.at){
        TokenSpan span=span_at(input.tokens,cursor);bytes+=span.end-span.start;cursor+=1u64;
    }
    handle heap=GetProcessHeap();address temporary=HeapAlloc(heap,0,bytes);
    if(address_bits(temporary)==0u64){parse_storage_error(state.error,StorageError(2,0));return empty_text();}
    u64 index=start;u64 output=0u64;
    while(index<state.at){
        TokenSpan token=span_at(input.tokens,index);u64 at=token.start;
        while(at<token.end){
            unsafe_position<u8>(unsafe_address(address_bits(temporary)+output))=u8(byte_at(input.source,at));
            output+=1u64;at+=1u64;
        }
        index+=1u64;
    }
    StorageText stored=text(input.storage,temporary,bytes);
    parse_storage_error(state.error,stored.error);
    if(HeapFree(heap,0,temporary)==0){
        state.error.cleanup=StorageError(3,GetLastError());
        if(state.error.kind==0){state.error.kind=386;state.error.span=Span(0u64,0u64);}
    }
    return stored.value;
}
Text qualified_text(read ParseInputs input,inout ParseState state,u32 separator,bool type_name) on cpu {
    u64 start=state.at;scan_qualified(input,state,separator,type_name);
    return scanned_text(input,state,start);
}
extern "C" Text parse_type_name(read ParseInputs input,inout ParseState state) on cpu {
    return qualified_text(input,state,46,true);
}

u32 stored_byte(read ParseInputs input,Text value,u64 index) on cpu {
    address bytes=input.source;
    if(value.kind!=0){bytes=view(input.storage).text;}
    return byte_at(bytes,value.range.start+index);
}
bool text_is<const N:u32>(read ParseInputs input,Text value,array<u8,N> expected) on cpu {
    if(value.range.end-value.range.start!=u64(N)){return false;}
    u32 i=0;
    while(i<N){if(stored_byte(input,value,u64(i))!=u32(expected[i])){return false;}i+=1;}
    return true;
}
ExprNode expression_node(read ParseInputs input,ExprRef value) on cpu {
    return unsafe_position<ExprNode>(unsafe_address(address_bits(view(input.storage).nodes)+value.offset));
}
TypeNode type_node(read ParseInputs input,TypeRef value) on cpu {
    return unsafe_position<TypeNode>(unsafe_address(address_bits(view(input.storage).nodes)+value.offset));
}
ExprRef publish_expression(read ParseInputs input,inout ParseState state,ExprNode value) on cpu {
    StorageIndex stored=put_expr(input.storage,value);
    parse_storage_error(state.error,stored.error);return ExprRef(stored.offset);
}
TypeRef publish_type(read ParseInputs input,inout ParseState state,TypeNode value) on cpu {
    StorageIndex stored=put_type(input.storage,value);
    parse_storage_error(state.error,stored.error);return TypeRef(stored.offset);
}
ShapeRef publish_shape(read ParseInputs input,inout ParseState state,ShapeNode value) on cpu {
    StorageIndex stored=put_shape(input.storage,value);
    parse_storage_error(state.error,stored.error);return ShapeRef(stored.offset);
}
ArgumentRef publish_argument(read ParseInputs input,inout ParseState state,ArgumentNode value) on cpu {
    StorageIndex stored=put_argument(input.storage,value);
    parse_storage_error(state.error,stored.error);return ArgumentRef(stored.offset);
}
TypeNode named_type(Text name) on cpu {
    return TypeNode(name,0,Slice(0u64,0u64),0,0,0,
        Optional<ShapeRef>(0,ShapeRef(0u64)),Optional<Slice>(0,Slice(0u64,0u64)));
}
// Decimal conversion here selects the original Shape::Literal alternative;
// other spellings remain the original expression, rather than being rejected.
ShapeNode expression_shape(read ParseInputs input,inout ParseState state,ExprNode expression) on cpu {
    match(expression.kind){
        ExprKind::Number(number){
            u64 length=number.range.end-number.range.start;
            bool literal=length!=0u64;u32 value=0;u64 i=0u64;
            while(i<length&&literal){
                u32 digit=stored_byte(input,number,i);
                if(digit<48||digit>57){literal=false;}
                else{
                    digit-=48;
                    if(value>429496729||value==429496729&&digit>5){literal=false;}
                    else{value=value*10+digit;}
                }
                i+=1u64;
            }
            if(literal){return ShapeNode::Literal(value);}
        }
        ExprKind::StaticU32(value){return ShapeNode::Literal(value);}
        ExprKind::Name(name){return ShapeNode::Constant(name);}
        ExprKind::Bool(value){}
        ExprKind::Member(value){}
        ExprKind::Index(value){}
        ExprKind::Call(value){}
        ExprKind::Invoke(value){}
        ExprKind::Construct(value){}
        ExprKind::Specialize(value){}
        ExprKind::FunctionRef(value){}
        ExprKind::Cooperate(value){}
        ExprKind::Target(value){}
        ExprKind::Unary(value){}
        ExprKind::Binary(value){}
        ExprKind::Select(value){}
    }
    return ShapeNode::Expression(publish_expression(input,state,expression));
}
// Precedence is shared by the ordinary expression and type-argument readers.
// Only the original type-argument caller ends before a > or >> token.
u32 operator_precedence(read ParseInputs input,u64 cursor) on cpu {
    u64 at=cursor;
    if(token_is(input,at,124,124)){return 1;}
    if(token_is(input,at,38,38)){return 2;}
    if(token_is(input,at,124,0)){return 3;}
    if(token_is(input,at,94,0)){return 4;}
    if(token_is(input,at,38,0)){return 5;}
    if(token_is(input,at,61,61)||token_is(input,at,33,61)){return 6;}
    if(token_is(input,at,60,0)||token_is(input,at,62,0)||token_is(input,at,60,61)||token_is(input,at,62,61)){return 7;}
    if(token_is(input,at,60,60)||token_is(input,at,62,62)){return 8;}
    if(token_is(input,at,43,0)||token_is(input,at,45,0)){return 9;}
    if(token_is(input,at,42,0)||token_is(input,at,47,0)||token_is(input,at,37,0)){return 10;}
    return 0;
}

extern "C" address HeapReAlloc(handle heap,u32 flags,address data,usize bytes) on cpu from "KERNEL32.dll";
struct TemporaryValues {address data;u64 count;u64 capacity;};
TemporaryValues temporary_values() on cpu {return TemporaryValues(unsafe_address(0u64),0u64,0u64);}
void parse_fail(inout ParseError failure,u32 kind,u32 detail,Span span) on cpu {
    if(failure.kind==0){failure.kind=kind;failure.detail=detail;failure.span=span;}
}
void temporary_free(inout ParseError failure,TemporaryValues values) on cpu {
    if(values.capacity!=0u64&&HeapFree(GetProcessHeap(),0,values.data)==0){
        if(failure.cleanup.kind==0){failure.cleanup=StorageError(3,GetLastError());}
        if(failure.kind==0){failure.kind=386;failure.span=Span(0u64,0u64);}
    }
}
void temporary_push<T>(inout ParseError failure,inout TemporaryValues values,T value) on cpu {
    u64 stride=size_of<T>();
    if(values.count==18446744073709551615u64/stride){parse_storage_error(failure,StorageError(1,0));return;}
    u64 required=values.count+1u64;
    if(required>values.capacity){
        u64 capacity=8u64;
        if(values.capacity!=0u64){
            capacity=required;
            if(values.capacity<=18446744073709551615u64/stride/2u64){capacity=values.capacity*2u64;}
        }
        address memory=unsafe_address(0u64);
        if(values.capacity==0u64){memory=HeapAlloc(GetProcessHeap(),0,capacity*stride);}
        else{memory=HeapReAlloc(GetProcessHeap(),0,values.data,capacity*stride);}
        if(address_bits(memory)==0u64){parse_storage_error(failure,StorageError(2,0));return;}
        values.data=memory;values.capacity=capacity;
    }
    unsafe_position<T>(unsafe_address(address_bits(values.data)+values.count*stride))=value;
    values.count=required;
}
T temporary_at<T>(read TemporaryValues values,u64 index) on cpu {
    return unsafe_position<T>(unsafe_address(address_bits(values.data)+index*size_of<T>()));
}
Slice finish_indices(read ParseInputs input,inout ParseState state,TemporaryValues values) on cpu {
    Slice result=Slice(0u64,0u64);
    if(state.error.kind==0&&values.count!=0u64){
        StorageSlice stored=indices(input.storage,values.data,values.count);
        parse_storage_error(state.error,stored.error);result=stored.value;
    }
    temporary_free(state.error,values);return result;
}
Slice finish_choices(read ParseInputs input,inout ParseState state,TemporaryValues values) on cpu {
    Slice result=Slice(0u64,0u64);
    if(state.error.kind==0){
        u64 at=0u64;
        while(at<values.count){
            u32 value=u32(temporary_at<u64>(values,at));
            unsafe_position<u32>(unsafe_address(address_bits(values.data)+at*4u64))=value;at+=1u64;
        }
        StorageSlice stored=choices(input.storage,values.data,values.count);
        parse_storage_error(state.error,stored.error);result=stored.value;
    }
    temporary_free(state.error,values);return result;
}
bool take_symbol(read ParseInputs input,inout u64 cursor,u32 first,u32 second) on cpu {
    if(token_is(input,cursor,first,second)){cursor+=1u64;return true;}
    return false;
}
Text current_text(read ParseInputs input,u64 cursor) on cpu {return Text(0,parse_span(input,cursor));}
bool word_is<const N:u32>(read ParseInputs input,u64 cursor,array<u8,N> expected) on cpu {
    return text_is(input,current_text(input,cursor),expected);
}
bool take_word<const N:u32>(read ParseInputs input,inout u64 cursor,array<u8,N> expected) on cpu {
    if(word_is(input,cursor,expected)){cursor+=1u64;return true;}return false;
}
bool expect_word<const N:u32>(read ParseInputs input,inout ParseState state,array<u8,N> expected) on cpu {
    if(take_word(input,state.at,expected)){return true;}
    parse_fail(state.error,10,0,parse_span(input,state.at));
    StorageText description=text(input.storage,address_of(expected),u64(N));
    parse_storage_error(state.error,description.error);state.error.expected=description.value;return false;
}
bool close_type(read ParseInputs input,inout ParseState state) on cpu {
    if(token_is(input,state.at,62,62)){
        address token=unsafe_address(address_bits(input.tokens)+state.at*16u64);
        unsafe_position<TokenSpan>(token).start+=1u64;return true;
    }
    return expect_word(input,state,">");
}
bool same_text(read ParseInputs input,Text left,Text right) on cpu {
    u64 length=left.range.end-left.range.start;
    if(length!=right.range.end-right.range.start){return false;}
    u64 at=0u64;
    while(at<length){if(stored_byte(input,left,at)!=stored_byte(input,right,at)){return false;}at+=1u64;}
    return true;
}
bool text_before(read ParseInputs input,Text left,Text right) on cpu {
    u64 a=left.range.end-left.range.start;u64 b=right.range.end-right.range.start;u64 at=0u64;
    while(at<a&&at<b){
        u32 x=stored_byte(input,left,at);u32 y=stored_byte(input,right,at);
        if(x!=y){return x<y;}at+=1u64;
    }
    return a<=b;
}
Text contract_value_path(read ParseInputs input,inout ParseState state) on cpu {
    u64 start=state.at;scan_qualified(input,state,46,false);
    while(state.error.kind==0&&take_symbol(input,state.at,91,0)){
        if(!expect_word(input,state,"*")||!expect_word(input,state,"]")){return empty_text();}
        if(take_symbol(input,state.at,46,0)){scan_qualified(input,state,46,false);}
    }
    return scanned_text(input,state,start);
}
bool function_clause(read ParseInputs input,u64 at) on cpu {
    if(at>=input.count){return false;}
    Text token=Text(0,parse_span(input,at));
    return (text_is(input,token,"requires")||text_is(input,token,"ensures"))&&token_is(input,at+2u64,40,0);
}
AngleEnd function_suffix_end(read ParseInputs input,u64 at) on cpu {
    u64 index=at;
    while(function_clause(input,index)){
        index+=3u64;u64 depth=1u64;
        while(depth!=0u64){
            if(index>=input.count){return AngleEnd(0u64,0);}
            if(token_is(input,index,40,0)){depth+=1u64;}
            else if(token_is(input,index,41,0)){depth-=1u64;}
            index+=1u64;
        }
    }
    return AngleEnd(index,1);
}
bool declaration_type(read ParseInputs input,u64 cursor) on cpu {
    if(word_is(input,cursor,"cooperate")){return false;}
    if((word_is(input,cursor,"view")||word_is(input,cursor,"tile"))&&token_is(input,cursor+1u64,60,0)){return true;}
    if(!identifier_at(input,cursor)){return false;}
    u64 at=cursor+1u64;
    while((token_is(input,at,46,0)||token_is(input,at,58,58))&&identifier_at(input,at+1u64)){at+=2u64;}
    if(token_is(input,at,60,0)){
        AngleEnd end=angle_end(input.source,input.tokens,input.count,at);if(end.present==0){return false;}at=end.end;
    }
    AngleEnd end=function_suffix_end(input,at);return end.present!=0&&identifier_at(input,end.end);
}
bool value_argument(read ParseInputs input,u64 cursor) on cpu {
    Text first=current_text(input,cursor);u64 length=first.range.end-first.range.start;
    u32 byte=0;if(length!=0u64){byte=stored_byte(input,first,0u64);}
    if(byte>=48&&byte<=57||byte==34||word_is(input,cursor,"(")||word_is(input,cursor,"+")||word_is(input,cursor,"-")||word_is(input,cursor,"!")||word_is(input,cursor,"~")||word_is(input,cursor,"true")||word_is(input,cursor,"false")){return true;}
    u64 at=cursor+1u64;
    while(token_is(input,at,46,0)||token_is(input,at,58,58)){at+=2u64;}
    if(token_is(input,at,60,0)){
        AngleEnd end=angle_end(input.source,input.tokens,input.count,at);if(end.present==0){return false;}at=end.end;
    }
    return token_is(input,at,40,0)||token_is(input,at,43,0)||token_is(input,at,45,0)||token_is(input,at,42,0)||token_is(input,at,47,0)||token_is(input,at,37,0)
        ||token_is(input,at,38,0)||token_is(input,at,124,0)||token_is(input,at,94,0)||token_is(input,at,60,60)||token_is(input,at,61,61)||token_is(input,at,33,61)
        ||token_is(input,at,60,61)||token_is(input,at,62,61)||token_is(input,at,38,38)||token_is(input,at,124,124)||token_is(input,at,63,0);
}

struct ContractDraft {u32 kind;PredicateNode predicate;Span span;};
ExprRef read_expression_ref(read ParseInputs input,inout ParseState state,u32 minimum,bool type_argument) on cpu {
    ExprNode expression=read_expression(input,state,minimum,type_argument);
    if(state.error.kind!=0){return ExprRef(0u64);}
    return publish_expression(input,state,expression);
}
ConditionRef source_condition(read ParseInputs input,inout ParseState state,ExprNode expression,u32 detail) on cpu {
    StorageCondition result=condition_input(input.storage,expression);parse_storage_error(state.error,result.error);
    if(state.error.kind==0&&result.present==0){parse_fail(state.error,225,detail,expression.span);}
    return result.value;
}
ShapeNode read_shape(read ParseInputs input,inout ParseState state) on cpu {
    ExprNode expression=read_expression(input,state,0,true);
    if(state.error.kind!=0){return ShapeNode::Literal(0);}
    return expression_shape(input,state,expression);
}
Slice generic_arguments(read ParseInputs input,inout ParseState state) on cpu {
    TemporaryValues arguments=temporary_values();
    if(!expect_word(input,state,"<")){return finish_indices(input,state,arguments);}
    bool more=true;
    while(more&&state.error.kind==0){
        ArgumentNode argument=ArgumentNode::Value(ShapeRef(0u64));
        if(value_argument(input,state.at)){
            ShapeNode value=read_shape(input,state);
            if(state.error.kind==0){argument=ArgumentNode::Value(publish_shape(input,state,value));}
        }else{
            TypeNode ty=read_type(input,state);
            if(state.error.kind==0){argument=ArgumentNode::Type(publish_type(input,state,ty));}
        }
        if(state.error.kind!=0){break;}
        ArgumentRef stored=publish_argument(input,state,argument);temporary_push(state.error,arguments,stored.offset);
        if(token_is(input,state.at,62,0)||token_is(input,state.at,62,62)){close_type(input,state);more=false;}
        else{expect_word(input,state,",");}
    }
    return finish_indices(input,state,arguments);
}
TypeNode read_type(read ParseInputs input,inout ParseState state) on cpu {
    Text name=parse_type_name(input,state);TypeNode result=named_type(name);
    if(state.error.kind!=0){return result;}
    bool dataset=text_is(input,name,"dataset");bool is_view=text_is(input,name,"view");bool tile=text_is(input,name,"tile");
    if(dataset||is_view||tile){
        if(!expect_word(input,state,"<")){return result;}
        result=read_type(input,state);if(state.error.kind!=0){return result;}
        if(result.dataset!=0||result.view!=0||result.tile!=0){parse_fail(state.error,232,0,parse_span(input,state.at));return result;}
        Optional<ShapeRef> count=Optional<ShapeRef>(0,ShapeRef(0u64));
        if(dataset&&take_symbol(input,state.at,44,0)){
            ShapeNode value=read_shape(input,state);
            if(state.error.kind!=0){return result;}
            count=Optional<ShapeRef>(1,publish_shape(input,state,value));
        }
        if(state.error.kind==0){close_type(input,state);}
        result.dataset=0;result.view=0;result.tile=0;
        if(dataset){result.dataset=1;}if(is_view){result.view=1;}if(tile){result.tile=1;}
        result.count=count;return result;
    }
    if(token_is(input,state.at,60,0)){result.arguments=generic_arguments(input,state);}
    if(state.error.kind==0&&text_is(input,name,"cfn")){result.contracts=function_contracts(input,state);}
    return result;
}
ContractDraft read_contract(read ParseInputs input,inout ParseState state) on cpu {
    Span span=parse_span(input,state.at);u32 kind=0;
    if(!take_word(input,state.at,"requires")){kind=1;expect_word(input,state,"ensures");}
    ContractDraft result=ContractDraft(kind,PredicateNode::Nonzero(empty_text()),span);
    if(state.error.kind!=0){return result;}
    Text predicate=parse_identifier(input,state);
    if(state.error.kind!=0||!expect_word(input,state,"(")){return result;}
    if(text_is(input,predicate,"prefix")){
        Text data=parse_identifier(input,state);if(state.error.kind!=0||!expect_word(input,state,",")){return result;}
        Text count=parse_identifier(input,state);if(state.error.kind!=0||!expect_word(input,state,".")){return result;}
        Text field=qualified_text(input,state,46,false);result.predicate=PredicateNode::Prefix(PrefixPredicate(data,count,field));
    }else if(text_is(input,predicate,"cleanup")){
        Text value=contract_value_path(input,state);if(state.error.kind!=0){return result;}
        ConditionRef condition=ConditionRef(0u64);
        if(take_symbol(input,state.at,44,0)){
            ExprNode expression=read_expression(input,state,0,false);
            if(state.error.kind!=0){return result;}condition=source_condition(input,state,expression,1);
        }else{
            StorageIndex stored=put_condition(input.storage,ConditionNode::Bool(1));parse_storage_error(state.error,stored.error);condition=ConditionRef(stored.offset);
        }
        result.predicate=PredicateNode::Cleanup(ConditionalPredicate(value,condition));
    }else if(text_is(input,predicate,"nonzero")){
        result.predicate=PredicateNode::Nonzero(contract_value_path(input,state));
    }else if(text_is(input,predicate,"nonempty")){
        result.predicate=PredicateNode::Nonempty(contract_value_path(input,state));
    }else if(text_is(input,predicate,"contiguous")){
        result.predicate=PredicateNode::Contiguous(qualified_text(input,state,46,false));
    }else if(text_is(input,predicate,"memory")){
        Optional<u32> access=Optional<u32>(0,0);
        if(take_word(input,state.at,"none")){}
        else if(take_word(input,state.at,"read")){access=Optional<u32>(1,1);}
        else if(take_word(input,state.at,"write")){access=Optional<u32>(1,2);}
        else if(take_word(input,state.at,"inout")){access=Optional<u32>(1,3);}
        else{parse_fail(state.error,225,2,span);return result;}
        result.predicate=PredicateNode::Memory(access);
    }else if(text_is(input,predicate,"initialized")){
        Text value=qualified_text(input,state,46,false);if(state.error.kind!=0||!expect_word(input,state,",")){return result;}
        ExprNode expression=read_expression(input,state,0,false);
        if(state.error.kind!=0){return result;}
        result.predicate=PredicateNode::Initialized(ConditionalPredicate(value,source_condition(input,state,expression,3)));
    }else if(text_is(input,predicate,"same_length")){
        Text left=qualified_text(input,state,46,false);if(state.error.kind!=0||!expect_word(input,state,",")){return result;}
        Text right=qualified_text(input,state,46,false);
        if(state.error.kind!=0){return result;}
        if(!text_before(input,left,right)){Text swap=left;left=right;right=swap;}
        result.predicate=PredicateNode::SameLength(SameLengthPredicate(left,right));
    }else if(text_is(input,predicate,"range")){
        Text value=contract_value_path(input,state);TemporaryValues intervals=temporary_values();
        while(state.error.kind==0&&take_symbol(input,state.at,44,0)){
            ExprNode lower=read_expression(input,state,0,false);
            if(state.error.kind!=0||!expect_word(input,state,",")){break;}
            ExprNode upper=read_expression(input,state,0,false);if(state.error.kind!=0){break;}
            ConditionRef lo=source_condition(input,state,lower,4);if(state.error.kind!=0){break;}
            ConditionRef hi=source_condition(input,state,upper,4);if(state.error.kind!=0){break;}
            StorageIndex stored=put_interval(input.storage,IntervalNode(lo,hi));parse_storage_error(state.error,stored.error);
            if(state.error.kind==0){temporary_push(state.error,intervals,stored.offset);}
        }
        result.predicate=PredicateNode::Range(RangePredicate(value,finish_indices(input,state,intervals)));
    }else if(text_is(input,predicate,"index")){
        Text value=contract_value_path(input,state);if(state.error.kind!=0||!expect_word(input,state,",")){return result;}
        result.predicate=PredicateNode::Index(IndexPredicate(value,contract_value_path(input,state)));
    }else{parse_fail(state.error,225,5,span);return result;}
    if(state.error.kind==0){expect_word(input,state,")");}
    return result;
}
Optional<Slice> function_contracts(read ParseInputs input,inout ParseState state) on cpu {
    TemporaryValues contracts=temporary_values();
    while(state.error.kind==0&&function_clause(input,state.at)){
        ContractDraft contract=read_contract(input,state);if(state.error.kind!=0){break;}
        bool accepted=false;
        match(contract.predicate){
            PredicateNode::Nonzero(value){accepted=contract.kind==0;}
            PredicateNode::Initialized(value){accepted=contract.kind==1;}
            PredicateNode::Memory(value){accepted=contract.kind==1;}
            PredicateNode::Prefix(value){}
            PredicateNode::Nonempty(value){}
            PredicateNode::Index(value){}
            PredicateNode::Contiguous(value){}
            PredicateNode::SameLength(value){}
            PredicateNode::Cleanup(value){}
            PredicateNode::Range(value){}
        }
        if(!accepted){parse_fail(state.error,225,6,contract.span);break;}
        StorageIndex stored=put_predicate(input.storage,contract.predicate);parse_storage_error(state.error,stored.error);
        if(state.error.kind==0){temporary_push(state.error,contracts,stored.offset);}
    }
    u32 present=0;if(contracts.count!=0u64){present=1;}
    return Optional<Slice>(present,finish_indices(input,state,contracts));
}
bool duplicate_parameter(read ParseInputs input,read TemporaryValues types,read TemporaryValues values,Text name) on cpu {
    u64 at=0u64;
    while(at<types.count){
        TypeParameterNode node=unsafe_position<TypeParameterNode>(unsafe_address(address_bits(view(input.storage).nodes)+temporary_at<u64>(types,at)));
        if(same_text(input,node.name,name)){return true;}at+=1u64;
    }
    at=0u64;
    while(at<values.count){
        ValueParameterNode node=unsafe_position<ValueParameterNode>(unsafe_address(address_bits(view(input.storage).nodes)+temporary_at<u64>(values,at)));
        if(same_text(input,node.name,name)){return true;}at+=1u64;
    }
    return false;
}
ParsedParameters read_parameters(read ParseInputs input,inout ParseState state) on cpu {
    TemporaryValues types=temporary_values();TemporaryValues values=temporary_values();
    bool more=take_symbol(input,state.at,60,0);
    while(more&&state.error.kind==0){
        Span span=parse_span(input,state.at);bool is_value=take_word(input,state.at,"const");Text name=parse_identifier(input,state);
        if(state.error.kind!=0){break;}
        if(duplicate_parameter(input,types,values,name)){parse_fail(state.error,231,0,span);break;}
        if(is_value){
            if(!expect_word(input,state,":")){break;}
            TypeNode ty=read_type(input,state);if(state.error.kind!=0){break;}
            if(!(text_is(input,ty.name,"u32")||text_is(input,ty.name,"uint"))||ty.arguments.count!=0u64||ty.dataset!=0||ty.view!=0||ty.tile!=0){parse_fail(state.error,235,1,span);break;}
            bool restricted=take_word(input,state.at,"in");TemporaryValues options=temporary_values();
            bool option_more=false;
            if(restricted&&expect_word(input,state,"{")){option_more=!take_symbol(input,state.at,125,0);}
            while(option_more&&state.error.kind==0){
                ShapeNode choice=read_shape(input,state);if(state.error.kind!=0){break;}
                match(choice){
                    ShapeNode::Literal(value){temporary_push(state.error,options,u64(value));}
                    ShapeNode::Constant(value){parse_fail(state.error,235,2,span);}
                    ShapeNode::Expression(value){parse_fail(state.error,235,2,span);}
                }
                if(state.error.kind!=0){break;}
                if(take_symbol(input,state.at,125,0)){option_more=false;}else{expect_word(input,state,",");}
            }
            Slice choices=finish_choices(input,state,options);
            if(state.error.kind!=0){break;}
            u32 present=0;if(restricted){present=1;}
            StorageIndex stored=put_value_parameter(input.storage,ValueParameterNode(name,Optional<Slice>(present,choices)));parse_storage_error(state.error,stored.error);
            if(state.error.kind==0){temporary_push(state.error,values,stored.offset);}
        }else{
            if(values.count!=0u64){parse_fail(state.error,235,3,span);break;}
            Optional<TypeFamilyRef> family=Optional<TypeFamilyRef>(0,TypeFamilyRef(0u64));
            if(take_symbol(input,state.at,58,0)){
                Text bound=parse_type_name(input,state);if(state.error.kind!=0){break;}
                TypeFamilyNode node=TypeFamilyNode::Named(bound);
                if(text_is(input,bound,"numeric")){node=TypeFamilyNode::Numeric();}
                else if(text_is(input,bound,"floating")){node=TypeFamilyNode::Floating();}
                else if(text_is(input,bound,"integer")){node=TypeFamilyNode::Integer();}
                else if(text_is(input,bound,"scalar_numeric")){node=TypeFamilyNode::ScalarNumeric();}
                else if(text_is(input,bound,"scalar_integer")){node=TypeFamilyNode::ScalarInteger();}
                StorageIndex stored=put_type_family(input.storage,node);parse_storage_error(state.error,stored.error);family=Optional<TypeFamilyRef>(1,TypeFamilyRef(stored.offset));
            }
            if(state.error.kind!=0){break;}
            StorageIndex stored=put_type_parameter(input.storage,TypeParameterNode(name,family));parse_storage_error(state.error,stored.error);
            if(state.error.kind==0){temporary_push(state.error,types,stored.offset);}
        }
        if(state.error.kind!=0){break;}
        if(take_symbol(input,state.at,62,0)){more=false;}else{expect_word(input,state,",");}
    }
    Slice type_refs=finish_indices(input,state,types);Slice value_refs=finish_indices(input,state,values);
    return ParsedParameters(type_refs,value_refs);
}

// A path query borrows the existing nodes. Only the completed namespace name
// enters text storage; recursive member prefixes have no stored consumer.
struct PathSize {u32 present;u64 bytes;};
PathSize expression_path_size(read ParseInputs input,ExprNode expression) on cpu {
    match(expression.kind){
        ExprKind::Name(value){return PathSize(1,value.range.end-value.range.start);}
        ExprKind::Member(value){
            PathSize base=expression_path_size(input,expression_node(input,value.base));
            if(base.present==0){return base;}
            return PathSize(1,base.bytes+1u64+value.field.range.end-value.field.range.start);
        }
        ExprKind::Number(value){}
        ExprKind::StaticU32(value){}
        ExprKind::Bool(value){}
        ExprKind::Index(value){}
        ExprKind::Call(value){}
        ExprKind::Invoke(value){}
        ExprKind::Construct(value){}
        ExprKind::Specialize(value){}
        ExprKind::FunctionRef(value){}
        ExprKind::Cooperate(value){}
        ExprKind::Target(value){}
        ExprKind::Unary(value){}
        ExprKind::Binary(value){}
        ExprKind::Select(value){}
    }
    return PathSize(0,0u64);
}
u64 write_expression_path(read ParseInputs input,ExprNode expression,address output,u64 start) on cpu {
    u64 at=start;
    match(expression.kind){
        ExprKind::Name(value){
            u64 length=value.range.end-value.range.start;u64 i=0u64;
            while(i<length){
                unsafe_position<u8>(unsafe_address(address_bits(output)+at))=u8(stored_byte(input,value,i));
                at+=1u64;i+=1u64;
            }
        }
        ExprKind::Member(value){
            at=write_expression_path(input,expression_node(input,value.base),output,at);
            unsafe_position<u8>(unsafe_address(address_bits(output)+at))=u8(46);at+=1u64;
            u64 length=value.field.range.end-value.field.range.start;u64 i=0u64;
            while(i<length){
                unsafe_position<u8>(unsafe_address(address_bits(output)+at))=u8(stored_byte(input,value.field,i));
                at+=1u64;i+=1u64;
            }
        }
        ExprKind::Number(value){}
        ExprKind::StaticU32(value){}
        ExprKind::Bool(value){}
        ExprKind::Index(value){}
        ExprKind::Call(value){}
        ExprKind::Invoke(value){}
        ExprKind::Construct(value){}
        ExprKind::Specialize(value){}
        ExprKind::FunctionRef(value){}
        ExprKind::Cooperate(value){}
        ExprKind::Target(value){}
        ExprKind::Unary(value){}
        ExprKind::Binary(value){}
        ExprKind::Select(value){}
    }
    return at;
}
Text namespace_text(read ParseInputs input,inout ParseState state,ExprNode prefix,u64 prefix_bytes,u64 tail) on cpu {
    u64 bytes=prefix_bytes;u64 token=tail;
    while(token<state.at){
        TokenSpan span=span_at(input.tokens,token);u64 count=span.end-span.start;
        if(count>18446744073709551615u64-bytes){parse_storage_error(state.error,StorageError(1,0));return empty_text();}
        bytes+=count;token+=1u64;
    }
    handle heap=GetProcessHeap();address output=HeapAlloc(heap,0,bytes);
    if(address_bits(output)==0u64){parse_storage_error(state.error,StorageError(2,0));return empty_text();}
    u64 at=write_expression_path(input,prefix,output,0u64);token=tail;
    while(token<state.at){
        TokenSpan span=span_at(input.tokens,token);u64 i=span.start;
        while(i<span.end){
            unsafe_position<u8>(unsafe_address(address_bits(output)+at))=u8(byte_at(input.source,i));at+=1u64;i+=1u64;
        }
        token+=1u64;
    }
    StorageText stored=text(input.storage,output,bytes);parse_storage_error(state.error,stored.error);
    if(HeapFree(heap,0,output)==0){
        if(state.error.cleanup.kind==0){state.error.cleanup=StorageError(3,GetLastError());}
        if(state.error.kind==0){state.error.kind=386;state.error.span=Span(0u64,0u64);}
    }
    return stored.value;
}
Slice expression_arguments(read ParseInputs input,inout ParseState state) on cpu {
    TemporaryValues arguments=temporary_values();
    if(!expect_word(input,state,"(")){return finish_indices(input,state,arguments);}
    bool more=!take_symbol(input,state.at,41,0);
    while(more&&state.error.kind==0){
        ExprRef value=read_expression_ref(input,state,0,false);if(state.error.kind!=0){break;}
        temporary_push(state.error,arguments,value.offset);
        if(take_symbol(input,state.at,41,0)){more=false;}else{expect_word(input,state,",");}
    }
    return finish_indices(input,state,arguments);
}
bool angle_before_call(read ParseInputs input,u64 at) on cpu {
    AngleEnd end=angle_end(input.source,input.tokens,input.count,at);
    return end.present!=0&&(token_is(input,end.end,40,0)||function_clause(input,end.end));
}
bool angle_before_reference(read ParseInputs input,u64 at) on cpu {
    AngleEnd end=angle_end(input.source,input.tokens,input.count,at);
    return end.present!=0&&(token_is(input,end.end,40,0)||token_is(input,end.end,44,0)||token_is(input,end.end,41,0)||token_is(input,end.end,59,0));
}
Text literal_text<const N:u32>(read ParseInputs input,inout ParseState state,array<u8,N> value) on cpu {
    StorageText stored=text(input.storage,address_of(value),u64(N));
    parse_storage_error(state.error,stored.error);return stored.value;
}
Text literal_decimal(read ParseInputs input,inout ParseState state,u64 value) on cpu {
    array<u8,20> digits=array<u8,20>(u8(0),u8(0),u8(0),u8(0),u8(0),u8(0),u8(0),u8(0),u8(0),u8(0),u8(0),u8(0),u8(0),u8(0),u8(0),u8(0),u8(0),u8(0),u8(0),u8(0));
    u32 at=20;u64 remaining=value;
    for(index in 0..20){
        u32 slot=19-index;digits[slot]=u8(48u64+remaining%10u64);at=slot;remaining/=10u64;
        if(remaining==0u64){break;}
    }
    StorageText stored=text(input.storage,unsafe_address(address_bits(address_of(digits))+u64(at)),20u64-u64(at));
    parse_storage_error(state.error,stored.error);return stored.value;
}
Slice literal_index(read ParseInputs input,inout ParseState state,u64 value) on cpu {
    StorageSlice stored=indices(input.storage,address_of(value),1u64);
    parse_storage_error(state.error,stored.error);return stored.value;
}
void literal_byte(read ParseInputs input,inout ParseState state,inout TemporaryValues values,TypeRef byte_type,Span span,u32 value) on cpu {
    Text digits=literal_decimal(input,state,u64(value));if(state.error.kind!=0){return;}
    ExprRef number=publish_expression(input,state,ExprNode(ExprKind::Number(digits),span));if(state.error.kind!=0){return;}
    Slice arguments=literal_index(input,state,number.offset);if(state.error.kind!=0){return;}
    ExprRef byte=publish_expression(input,state,ExprNode(ExprKind::Construct(ConstructExpr(byte_type,arguments)),span));
    if(state.error.kind==0){temporary_push(state.error,values,byte.offset);}
}
ExprNode ordinary_literal(read ParseInputs input,inout ParseState state,Span span) on cpu {
    ExprNode result=ExprNode(ExprKind::Name(empty_text()),span);
    TemporaryValues values=temporary_values();
    Text byte_name=literal_text(input,state,"u8");
    TypeRef byte_type=publish_type(input,state,named_type(byte_name));
    u64 at=span.start+1u64;u64 end=span.end-1u64;
    while(at<end&&state.error.kind==0){
        u32 value=byte_at(input.source,at);at+=1u64;
        if(value!=92){literal_byte(input,state,values,byte_type,span,value);}
        else if(at==end){parse_fail(state.error,385,4,span);}
        else{
            u32 escape=byte_at(input.source,at);at+=1u64;
            if(escape==92||escape==34){literal_byte(input,state,values,byte_type,span,escape);}
            else if(escape==110){literal_byte(input,state,values,byte_type,span,10);}
            else if(escape==114){literal_byte(input,state,values,byte_type,span,13);}
            else if(escape==116){literal_byte(input,state,values,byte_type,span,9);}
            else if(escape==48){literal_byte(input,state,values,byte_type,span,0);}
            else if(escape==117){
                if(at==end||byte_at(input.source,at)!=123){parse_fail(state.error,385,1,span);}
                else{
                    at+=1u64;u64 close=at;
                    while(close<end&&byte_at(input.source,close)!=125){close+=1u64;}
                    if(close==end){parse_fail(state.error,385,2,span);}
                    else{
                        bool valid=at!=close;u32 scalar=0;
                        while(at<close&&valid){
                            u32 digit=byte_at(input.source,at);at+=1u64;
                            if(digit>=48&&digit<=57){digit-=48;}
                            else if(digit>=65&&digit<=70){digit=digit-65+10;}
                            else if(digit>=97&&digit<=102){digit=digit-97+10;}
                            else{valid=false;}
                            if(valid){
                                if(scalar>268435455){valid=false;}
                                else{scalar=scalar*16+digit;}
                            }
                        }
                        if(!valid||!is_scalar(scalar)){parse_fail(state.error,385,3,span);}
                        else{
                            Utf8Units units=utf8_units(scalar);u32 index=0;
                            while(index<units.length&&state.error.kind==0){
                                literal_byte(input,state,values,byte_type,span,u32(u8(units.bits>>(index*8))));index+=1;
                            }
                            at=close+1u64;
                        }
                    }
                }
            }else{parse_fail(state.error,385,4,span);}
        }
    }
    u64 length=values.count;Slice elements=finish_indices(input,state,values);
    if(state.error.kind!=0){return result;}
    ArgumentRef element=publish_argument(input,state,ArgumentNode::Type(byte_type));
    Text digits=literal_decimal(input,state,length);
    ExprRef count_value=publish_expression(input,state,ExprNode(ExprKind::Number(digits),span));
    ShapeRef count_shape=publish_shape(input,state,ShapeNode::Expression(count_value));
    ArgumentRef count=publish_argument(input,state,ArgumentNode::Value(count_shape));
    array<u64,2> arguments=array<u64,2>(element.offset,count.offset);
    StorageSlice stored=indices(input.storage,address_of(arguments),2u64);parse_storage_error(state.error,stored.error);
    TypeNode array_type=named_type(literal_text(input,state,"array"));array_type.arguments=stored.value;
    TypeRef ty=publish_type(input,state,array_type);
    result.kind=ExprKind::Construct(ConstructExpr(ty,elements));return result;
}

ExprNode read_expression(read ParseInputs input,inout ParseState state,u32 minimum,bool type_argument) on cpu {
    Span span=parse_span(input,state.at);ExprNode left=ExprNode(ExprKind::Name(empty_text()),span);
    if(word_is(input,state.at,"-")||word_is(input,state.at,"+")||word_is(input,state.at,"!")||word_is(input,state.at,"~")){
        Text operation=current_text(input,state.at);state.at+=1u64;
        ExprRef value=read_expression_ref(input,state,12,type_argument);
        left.kind=ExprKind::Unary(UnaryExpr(operation,value));
    }else if(word_is(input,state.at,"target")&&token_is(input,state.at+1u64,40,0)){
        state.at+=2u64;Text target=parse_identifier(input,state);u32 selected=0;
        if(state.error.kind!=0){return left;}
        if(text_is(input,target,"cpu")){selected=1;}
        else if(text_is(input,target,"gpu")){selected=2;}
        else{parse_fail(state.error,266,0,span);return left;}
        if(!expect_word(input,state,",")){return left;}
        ExprRef value=read_expression_ref(input,state,0,false);
        if(state.error.kind==0){expect_word(input,state,")");}
        left.kind=ExprKind::Target(TargetExpr(selected,value));
    }else if(take_word(input,state.at,"cooperate")){
        Text name=qualified_text(input,state,46,true);if(state.error.kind!=0){return left;}
        Optional<Slice> types=Optional<Slice>(0,Slice(0u64,0u64));
        if(token_is(input,state.at,60,0)){types=Optional<Slice>(1,generic_arguments(input,state));}
        if(state.error.kind!=0){return left;}
        Slice arguments=expression_arguments(input,state);
        left.kind=ExprKind::Cooperate(CooperateExpr(name,types,arguments));
    }else if(word_is(input,state.at,"cfn")&&token_is(input,state.at+1u64,60,0)&&angle_before_call(input,state.at+1u64)){
        TypeNode ty=read_type(input,state);if(state.error.kind!=0){return left;}
        Slice arguments=expression_arguments(input,state);if(state.error.kind!=0){return left;}
        left.kind=ExprKind::Construct(ConstructExpr(publish_type(input,state,ty),arguments));
    }else if(take_symbol(input,state.at,40,0)){
        left=read_expression(input,state,0,false);
        if(state.error.kind==0){expect_word(input,state,")");}
    }else if(take_word(input,state.at,"true")){
        left.kind=ExprKind::Bool(1);
    }else if(take_word(input,state.at,"false")){
        left.kind=ExprKind::Bool(0);
    }else{
        Text token=current_text(input,state.at);u64 length=token.range.end-token.range.start;u32 first=0;
        if(length!=0u64){first=stored_byte(input,token,0u64);}
        if(first==34){
            left=ordinary_literal(input,state,span);
            if(state.error.kind!=0){return left;}
            state.at+=1u64;
        }else if(first>=48&&first<=57||first==46){
            left.kind=ExprKind::Number(token);state.at+=1u64;
        }else{left.kind=ExprKind::Name(qualified_text(input,state,58,false));}
    }
    while(state.error.kind==0){
        if(take_symbol(input,state.at,58,58)){
            PathSize path=expression_path_size(input,left);
            if(path.present==0){parse_fail(state.error,14,0,span);break;}
            u64 tail=state.at-1u64;parse_identifier(input,state);
            while(state.error.kind==0&&take_symbol(input,state.at,58,58)){parse_identifier(input,state);}
            if(state.error.kind!=0){break;}
            left=ExprNode(ExprKind::Name(namespace_text(input,state,left,path.bytes,tail)),span);continue;
        }
        if(take_symbol(input,state.at,46,0)){
            Text field=parse_identifier(input,state);if(state.error.kind!=0){break;}
            left=ExprNode(ExprKind::Member(MemberExpr(publish_expression(input,state,left),field)),span);continue;
        }
        if(take_symbol(input,state.at,91,0)){
            ExprRef index=read_expression_ref(input,state,0,false);
            if(state.error.kind!=0||!expect_word(input,state,"]")){break;}
            left=ExprNode(ExprKind::Index(IndexExpr(publish_expression(input,state,left),index)),span);continue;
        }
        bool reference=false;Text reference_name=empty_text();
        match(left.kind){
            ExprKind::Name(name){reference=token_is(input,state.at,60,0)&&angle_before_reference(input,state.at);reference_name=name;}
            ExprKind::Number(value){}
            ExprKind::StaticU32(value){}
            ExprKind::Bool(value){}
            ExprKind::Member(value){}
            ExprKind::Index(value){}
            ExprKind::Call(value){}
            ExprKind::Invoke(value){}
            ExprKind::Construct(value){}
            ExprKind::Specialize(value){}
            ExprKind::FunctionRef(value){}
            ExprKind::Cooperate(value){}
            ExprKind::Target(value){}
            ExprKind::Unary(value){}
            ExprKind::Binary(value){}
            ExprKind::Select(value){}
        }
        if(reference){
            Slice arguments=generic_arguments(input,state);
            left=ExprNode(ExprKind::FunctionRef(FunctionExpr(reference_name,arguments)),span);continue;
        }
        if(token_is(input,state.at,40,0)){
            Slice arguments=expression_arguments(input,state);if(state.error.kind!=0){break;}
            ExprKind kind=ExprKind::Bool(0);
            bool named=false;
            match(left.kind){
                ExprKind::Name(name){kind=ExprKind::Call(CallExpr(name,arguments));named=true;}
                ExprKind::FunctionRef(value){kind=ExprKind::Specialize(SpecializeExpr(value.name,value.types,arguments));named=true;}
                ExprKind::Number(value){}
                ExprKind::StaticU32(value){}
                ExprKind::Bool(value){}
                ExprKind::Member(value){}
                ExprKind::Index(value){}
                ExprKind::Call(value){}
                ExprKind::Invoke(value){}
                ExprKind::Construct(value){}
                ExprKind::Specialize(value){}
                ExprKind::Cooperate(value){}
                ExprKind::Target(value){}
                ExprKind::Unary(value){}
                ExprKind::Binary(value){}
                ExprKind::Select(value){}
            }
            if(!named){kind=ExprKind::Invoke(InvokeExpr(publish_expression(input,state,ExprNode(left.kind,span)),arguments));}
            left=ExprNode(kind,span);continue;
        }
        if(minimum==0&&take_symbol(input,state.at,63,0)){
            ExprRef yes=read_expression_ref(input,state,0,false);
            if(state.error.kind!=0||!expect_word(input,state,":")){break;}
            ExprRef no=read_expression_ref(input,state,0,type_argument);
            if(state.error.kind!=0){break;}
            left=ExprNode(ExprKind::Select(SelectExpr(publish_expression(input,state,left),yes,no)),span);continue;
        }
        if(type_argument&&(token_is(input,state.at,62,0)||token_is(input,state.at,62,62))){break;}
        u32 precedence=operator_precedence(input,state.at);if(precedence==0||precedence<minimum){break;}
        Text operation=current_text(input,state.at);state.at+=1u64;
        ExprRef right=read_expression_ref(input,state,precedence+1,type_argument);
        if(state.error.kind!=0){break;}
        left=ExprNode(ExprKind::Binary(BinaryExpr(operation,publish_expression(input,state,left),right)),span);
    }
    return left;
}
extern "C" ExprRef parse_expression(read ParseInputs input,inout ParseState state,u32 minimum,u32 type_argument) on cpu {
    return read_expression_ref(input,state,minimum,type_argument!=0);
}
extern "C" TypeRef parse_type(read ParseInputs input,inout ParseState state) on cpu {
    TypeNode value=read_type(input,state);if(state.error.kind!=0){return TypeRef(0u64);}
    return publish_type(input,state,value);
}
extern "C" ContractRef parse_contract(read ParseInputs input,inout ParseState state) on cpu {
    ContractDraft value=read_contract(input,state);if(state.error.kind!=0){return ContractRef(0u64);}
    StorageIndex predicate=put_predicate(input.storage,value.predicate);parse_storage_error(state.error,predicate.error);
    if(state.error.kind!=0){return ContractRef(0u64);}
    StorageIndex stored=put_contract(input.storage,ContractNode(value.kind,PredicateRef(predicate.offset),value.span));
    parse_storage_error(state.error,stored.error);return ContractRef(stored.offset);
}
extern "C" ParsedParameters parse_parameters(read ParseInputs input,inout ParseState state) on cpu {
    return read_parameters(input,state);
}
extern "C" u32 looks_like_declaration(read ParseInputs input,read ParseState state) on cpu {
    if(declaration_type(input,state.at)){return 1;}return 0;
}

extern "C" ShapeRef parse_shape(read ParseInputs input,inout ParseState state) on cpu {
    ShapeNode value=read_shape(input,state);
    if(state.error.kind!=0){return ShapeRef(0u64);}
    return publish_shape(input,state,value);
}

StmtRef publish_statement(read ParseInputs input,inout ParseState state,StmtKind kind,Span span) on cpu {
    if(state.error.kind!=0){return StmtRef(0u64);}
    StorageIndex stored=put_stmt(input.storage,StmtNode(kind,span));
    parse_storage_error(state.error,stored.error);return StmtRef(stored.offset);
}

Slice read_block(read ParseInputs input,inout ParseState state) on cpu {
    if(!expect_word(input,state,"{")){return Slice(0u64,0u64);}
    TemporaryValues body=temporary_values();
    while(state.error.kind==0&&!take_symbol(input,state.at,125,0)){
        if(word_is(input,state.at,"")){parse_fail(state.error,13,0,parse_span(input,state.at));break;}
        StmtRef statement=read_statement(input,state);
        if(state.error.kind==0){temporary_push(state.error,body,statement.offset);}
    }
    return finish_indices(input,state,body);
}

StmtRef read_statement(read ParseInputs input,inout ParseState state) on cpu {
    Span span=parse_span(input,state.at);
    if(take_word(input,state.at,"parallel")){
        if(!expect_word(input,state,"(")){return StmtRef(0u64);}
        Text name=parse_identifier(input,state);
        if(state.error.kind!=0||!expect_word(input,state,"in")){return StmtRef(0u64);}
        bool selected=word_is(input,state.at,"where")&&token_is(input,state.at+1u64,40,0);
        if(selected){state.at+=2u64;}
        bool prefix=word_is(input,state.at,"prefix")&&token_is(input,state.at+1u64,40,0);
        if(prefix){state.at+=2u64;}
        Text dataset=parse_identifier(input,state);
        Optional<ExprRef> count=Optional<ExprRef>(0,ExprRef(0u64));
        Optional<ExprRef> selection=Optional<ExprRef>(0,ExprRef(0u64));
        if(state.error.kind!=0){return StmtRef(0u64);}
        if(prefix){
            if(!expect_word(input,state,",")){return StmtRef(0u64);}
            count=Optional<ExprRef>(1,read_expression_ref(input,state,0,false));
            if(state.error.kind!=0||!expect_word(input,state,")")){return StmtRef(0u64);}
        }
        if(selected){
            if(!expect_word(input,state,",")){return StmtRef(0u64);}
            selection=Optional<ExprRef>(1,read_expression_ref(input,state,0,false));
            if(state.error.kind!=0||!expect_word(input,state,")")){return StmtRef(0u64);}
        }
        if(!expect_word(input,state,")")){return StmtRef(0u64);}
        Slice body=read_block(input,state);
        if(state.error.kind!=0){return StmtRef(0u64);}
        StorageIndex domain=put_parallel(input.storage,ParallelNode(dataset,count,selection));
        parse_storage_error(state.error,domain.error);
        return publish_statement(input,state,StmtKind::Parallel(ParallelStmt(name,ParallelRef(domain.offset),body)),span);
    }
    if(take_word(input,state.at,"if")){
        if(!expect_word(input,state,"(")){return StmtRef(0u64);}
        ExprRef condition=read_expression_ref(input,state,0,false);
        if(state.error.kind!=0||!expect_word(input,state,")")){return StmtRef(0u64);}
        Slice yes=read_block(input,state);
        if(state.error.kind!=0){return StmtRef(0u64);}
        Slice no=Slice(0u64,0u64);
        if(take_word(input,state.at,"else")){
            if(word_is(input,state.at,"if")){
                StmtRef alternate=read_statement(input,state);
                if(state.error.kind!=0){return StmtRef(0u64);}
                StorageSlice stored=indices(input.storage,address_of(alternate.offset),1u64);
                parse_storage_error(state.error,stored.error);no=stored.value;
            }else{no=read_block(input,state);}
        }
        return publish_statement(input,state,StmtKind::If(IfStmt(condition,yes,no)),span);
    }
    if(take_word(input,state.at,"match")){
        if(!expect_word(input,state,"(")){return StmtRef(0u64);}
        ExprRef value=read_expression_ref(input,state,0,false);
        if(state.error.kind!=0||!expect_word(input,state,")")||!expect_word(input,state,"{")){return StmtRef(0u64);}
        TemporaryValues arms=temporary_values();
        while(state.error.kind==0&&!take_symbol(input,state.at,125,0)){
            Span at=parse_span(input,state.at);Text variant=parse_type_name(input,state);
            if(state.error.kind!=0||!expect_word(input,state,"(")){break;}
            Optional<Text> binding=Optional<Text>(0,empty_text());
            if(!take_symbol(input,state.at,41,0)){
                binding=Optional<Text>(1,parse_identifier(input,state));
                if(state.error.kind!=0||!expect_word(input,state,")")){break;}
            }
            Slice body=read_block(input,state);
            if(state.error.kind!=0){break;}
            StorageIndex arm=put_arm(input.storage,ArmNode(variant,binding,body,at));
            parse_storage_error(state.error,arm.error);
            if(state.error.kind==0){temporary_push(state.error,arms,arm.offset);}
        }
        Slice stored=finish_indices(input,state,arms);
        return publish_statement(input,state,StmtKind::Match(MatchStmt(value,stored)),span);
    }
    if(take_word(input,state.at,"while")){
        if(!expect_word(input,state,"(")){return StmtRef(0u64);}
        ExprRef condition=read_expression_ref(input,state,0,false);
        if(state.error.kind!=0||!expect_word(input,state,")")){return StmtRef(0u64);}
        Slice body=read_block(input,state);
        return publish_statement(input,state,StmtKind::While(WhileStmt(condition,body)),span);
    }
    if(take_word(input,state.at,"for")){
        if(!expect_word(input,state,"(")){return StmtRef(0u64);}
        Text name=parse_identifier(input,state);
        if(state.error.kind!=0||!expect_word(input,state,"in")){return StmtRef(0u64);}
        ExprRef start=read_expression_ref(input,state,0,false);
        if(state.error.kind!=0){return StmtRef(0u64);}
        Optional<ExprRef> end=Optional<ExprRef>(0,ExprRef(0u64));
        if(take_symbol(input,state.at,46,46)){
            end=Optional<ExprRef>(1,read_expression_ref(input,state,0,false));
        }
        if(state.error.kind!=0||!expect_word(input,state,")")){return StmtRef(0u64);}
        Slice body=read_block(input,state);
        return publish_statement(input,state,StmtKind::For(ForStmt(name,start,end,body)),span);
    }
    if(take_word(input,state.at,"return")){
        Optional<ExprRef> value=Optional<ExprRef>(0,ExprRef(0u64));
        if(!take_symbol(input,state.at,59,0)){
            value=Optional<ExprRef>(1,read_expression_ref(input,state,0,false));
            if(state.error.kind!=0||!expect_word(input,state,";")){return StmtRef(0u64);}
        }
        return publish_statement(input,state,StmtKind::Return(value),span);
    }
    if(take_word(input,state.at,"break")){
        expect_word(input,state,";");return publish_statement(input,state,StmtKind::Break(),span);
    }
    if(take_word(input,state.at,"continue")){
        expect_word(input,state,";");return publish_statement(input,state,StmtKind::Continue(),span);
    }
    if(word_is(input,state.at,"phase")&&token_is(input,state.at+1u64,59,0)){
        state.at+=2u64;return publish_statement(input,state,StmtKind::Phase(),span);
    }
    if(take_word(input,state.at,"seek")){
        if(!expect_word(input,state,"(")){return StmtRef(0u64);}
        Text name=parse_identifier(input,state);
        if(state.error.kind!=0||!expect_word(input,state,",")){return StmtRef(0u64);}
        ExprRef position=read_expression_ref(input,state,0,false);
        if(state.error.kind!=0||!expect_word(input,state,")")||!expect_word(input,state,";")){return StmtRef(0u64);}
        return publish_statement(input,state,StmtKind::Seek(SeekStmt(name,position)),span);
    }
    if(take_word(input,state.at,"let")){
        Text name=parse_identifier(input,state);
        if(state.error.kind!=0||!expect_word(input,state,"=")){return StmtRef(0u64);}
        ExprRef value=read_expression_ref(input,state,0,false);
        if(state.error.kind!=0||!expect_word(input,state,";")){return StmtRef(0u64);}
        return publish_statement(input,state,StmtKind::Let(LetStmt(Optional<TypeRef>(0,TypeRef(0u64)),name,value)),span);
    }
    if(declaration_type(input,state.at)){
        TypeRef ty=parse_type(input,state);
        if(state.error.kind!=0){return StmtRef(0u64);}
        Text name=parse_identifier(input,state);
        if(state.error.kind!=0){return StmtRef(0u64);}
        if(take_symbol(input,state.at,59,0)){
            return publish_statement(input,state,StmtKind::Declare(DeclareStmt(ty,name)),span);
        }
        if(!expect_word(input,state,"=")){return StmtRef(0u64);}
        ExprRef value=read_expression_ref(input,state,0,false);
        if(state.error.kind!=0||!expect_word(input,state,";")){return StmtRef(0u64);}
        return publish_statement(input,state,StmtKind::Let(LetStmt(Optional<TypeRef>(1,ty),name,value)),span);
    }
    ExprRef left=read_expression_ref(input,state,0,false);
    if(state.error.kind!=0){return StmtRef(0u64);}
    bool assignment=word_is(input,state.at,"=")||word_is(input,state.at,"+=")||word_is(input,state.at,"-=")
        ||word_is(input,state.at,"*=")||word_is(input,state.at,"/=")||word_is(input,state.at,"%=")
        ||word_is(input,state.at,"&=")||word_is(input,state.at,"|=")||word_is(input,state.at,"^=")
        ||word_is(input,state.at,"<<=")||word_is(input,state.at,">>=");
    if(assignment){
        Text operation=current_text(input,state.at);state.at+=1u64;
        ExprRef right=read_expression_ref(input,state,0,false);
        if(state.error.kind!=0||!expect_word(input,state,";")){return StmtRef(0u64);}
        return publish_statement(input,state,StmtKind::Assign(AssignStmt(left,operation,right)),span);
    }
    expect_word(input,state,";");return publish_statement(input,state,StmtKind::Expr(left),span);
}


Text stored_word<const N:u32>(read ParseInputs input,inout ParseState state,array<u8,N> value) on cpu {
    StorageText stored=text(input.storage,address_of(value),u64(N));
    parse_storage_error(state.error,stored.error);return stored.value;
}
Text read_link_string(read ParseInputs input,inout ParseState state) on cpu {
    Span span=parse_span(input,state.at);
    if(span.start==span.end||byte_at(input.source,span.start)!=34){parse_fail(state.error,263,1,span);return empty_text();}
    u64 length=span.end-span.start-2u64;
    address memory=HeapAlloc(GetProcessHeap(),0,length);
    if(address_bits(memory)==0u64){parse_storage_error(state.error,StorageError(2,0));return empty_text();}
    LinkStringResult decoded=decode_link_string(unsafe_address(address_bits(input.source)+span.start+1u64),length,memory);
    Text result=empty_text();
    if(decoded.error==0){
        StorageText stored=text(input.storage,memory,decoded.count);
        parse_storage_error(state.error,stored.error);result=stored.value;state.at+=1u64;
    }else{parse_fail(state.error,263,decoded.error+1,span);}
    temporary_free(state.error,TemporaryValues(memory,0u64,1u64));return result;
}
Slice finish_texts(read ParseInputs input,inout ParseState state,TemporaryValues values) on cpu {
    Slice result=Slice(0u64,0u64);
    if(state.error.kind==0&&values.count!=0u64){
        StorageSlice stored=texts(input.storage,values.data,values.count);
        parse_storage_error(state.error,stored.error);result=stored.value;
    }
    temporary_free(state.error,values);return result;
}
FunctionRef publish_function(read ParseInputs input,inout ParseState state,FunctionNode value) on cpu {
    StorageIndex stored=put_function(input.storage,value);
    parse_storage_error(state.error,stored.error);return FunctionRef(stored.offset);
}
FunctionNode function_node(Text name,TypeRef result,Span span) on cpu {
    return FunctionNode(name,Slice(0u64,0u64),Slice(0u64,0u64),Slice(0u64,0u64),result,
        0,0,Slice(0u64,0u64),Optional<Slice>(0,Slice(0u64,0u64)),0,
        Optional<ExternalRef>(0,ExternalRef(0u64)),span.end,span);
}
Slice generated_return(read ParseInputs input,inout ParseState state,ExprRef value,Span span) on cpu {
    StmtRef statement=publish_statement(input,state,StmtKind::Return(Optional<ExprRef>(1,value)),span);
    if(state.error.kind!=0){return Slice(0u64,0u64);}
    StorageSlice body=indices(input.storage,address_of(statement.offset),1u64);
    parse_storage_error(state.error,body.error);return body.value;
}
bool duplicate_function_parameter(read ParseInputs input,read TemporaryValues params,Text name) on cpu {
    u64 at=0u64;
    while(at<params.count){
        u64 offset=temporary_at<u64>(params,at);
        ParamNode parameter=unsafe_position<ParamNode>(unsafe_address(address_bits(view(input.storage).nodes)+offset));
        if(same_text(input,parameter.name,name)){return true;}at+=1u64;
    }
    return false;
}
FunctionRef read_function(read ParseInputs input,inout ParseState state,Span span) on cpu {
    u32 c_abi=0;u32 kind=0;
    if(take_word(input,state.at,"extern")){
        Text abi=read_link_string(input,state);
        if(state.error.kind!=0){return FunctionRef(0u64);}
        if(!text_is(input,abi,"C")){parse_fail(state.error,263,5,span);return FunctionRef(0u64);}c_abi=1;
    }
    if(take_word(input,state.at,"cooperative")){kind=1;}
    TypeRef result=parse_type(input,state);
    if(state.error.kind!=0){return FunctionRef(0u64);}
    Text name=parse_identifier(input,state);
    if(state.error.kind!=0){return FunctionRef(0u64);}
    ParsedParameters parameters=parse_parameters(input,state);
    if(state.error.kind!=0||!expect_word(input,state,"(")){return FunctionRef(0u64);}
    TemporaryValues params=temporary_values();
    bool more=!take_word(input,state.at,")");
    while(more&&state.error.kind==0){
        Span at=parse_span(input,state.at);u32 access=0;
        if(take_word(input,state.at,"read")){access=1;}
        else if(take_word(input,state.at,"write")){access=2;}
        else if(take_word(input,state.at,"inout")){access=3;}
        else if(take_word(input,state.at,"atomic")){access=4;}
        TypeRef ty=parse_type(input,state);
        if(state.error.kind!=0){break;}
        Text parameter=parse_identifier(input,state);if(state.error.kind!=0){break;}
        if(duplicate_function_parameter(input,params,parameter)){
            parse_fail(state.error,100,0,at);state.error.expected=parameter;break;
        }
        StorageIndex stored=put_param(input.storage,ParamNode(parameter,ty,access,at));
        parse_storage_error(state.error,stored.error);
        if(state.error.kind!=0){break;}
        temporary_push(state.error,params,stored.offset);
        if(take_word(input,state.at,")")){more=false;}else{expect_word(input,state,",");}
    }
    Slice param_refs=finish_indices(input,state,params);
    if(state.error.kind!=0){return FunctionRef(0u64);}
    u32 target=0;
    if(take_word(input,state.at,"on")){
        Text value=parse_identifier(input,state);if(state.error.kind!=0){return FunctionRef(0u64);}
        if(text_is(input,value,"cpu")){target=1;}
        else if(text_is(input,value,"gpu")){target=2;}
        else if(!text_is(input,value,"compute")){parse_fail(state.error,12,0,span);return FunctionRef(0u64);}
    }
    TemporaryValues contracts=temporary_values();
    while(state.error.kind==0&&(word_is(input,state.at,"requires")||word_is(input,state.at,"ensures"))){
        ContractRef contract=parse_contract(input,state);
        if(state.error.kind==0){temporary_push(state.error,contracts,contract.offset);}
    }
    Slice contract_refs=finish_indices(input,state,contracts);
    if(state.error.kind!=0){return FunctionRef(0u64);}
    Optional<ExternalRef> external=Optional<ExternalRef>(0,ExternalRef(0u64));
    if(c_abi!=0&&word_is(input,state.at,"from")){
        if(parameters.types.count!=0u64||parameters.values.count!=0u64){parse_fail(state.error,263,6,span);return FunctionRef(0u64);}
        state.at+=1u64;Text library=read_link_string(input,state);
        if(state.error.kind!=0){return FunctionRef(0u64);}
        Text symbol=name;
        if(take_word(input,state.at,"as")){symbol=read_link_string(input,state);}
        if(state.error.kind!=0){return FunctionRef(0u64);}
        StorageIndex stored=put_external(input.storage,ExternalNode(library,symbol));
        parse_storage_error(state.error,stored.error);external=Optional<ExternalRef>(1,ExternalRef(stored.offset));
    }
    if(state.error.kind!=0){return FunctionRef(0u64);}
    u64 signature_end=parse_span(input,state.at).start;
    Optional<Slice> body=Optional<Slice>(0,Slice(0u64,0u64));
    if(!take_word(input,state.at,";")){body=Optional<Slice>(1,read_block(input,state));}
    if(state.error.kind!=0){return FunctionRef(0u64);}
    return publish_function(input,state,FunctionNode(name,parameters.types,parameters.values,param_refs,result,
        kind,target,contract_refs,body,c_abi,external,signature_end,Span(span.start,parse_span(input,state.at-1u64).end)));
}
RecordRef read_record(read ParseInputs input,inout ParseState state,Span span,bool enumeration,inout TemporaryValues functions) on cpu {
    Text name=parse_identifier(input,state);if(state.error.kind!=0){return RecordRef(0u64);}
    ParsedParameters parameters=parse_parameters(input,state);
    Optional<TypeRef> representation=Optional<TypeRef>(0,TypeRef(0u64));
    Optional<ShapeRef> stride=Optional<ShapeRef>(0,ShapeRef(0u64));
    if(state.error.kind!=0){return RecordRef(0u64);}
    if(enumeration){
        if(!expect_word(input,state,":")){return RecordRef(0u64);}
        representation=Optional<TypeRef>(1,parse_type(input,state));
    }else if(take_word(input,state.at,"stride")){
        if(!expect_word(input,state,"(")){return RecordRef(0u64);}
        stride=Optional<ShapeRef>(1,parse_shape(input,state));
        if(state.error.kind!=0||!expect_word(input,state,")")){return RecordRef(0u64);}
    }
    if(state.error.kind!=0||!expect_word(input,state,"{")){return RecordRef(0u64);}
    TemporaryValues fields=temporary_values();TemporaryValues variants=temporary_values();
    bool payloads=parameters.types.count!=0u64||parameters.values.count!=0u64;
    while(state.error.kind==0&&!take_word(input,state.at,"}")){
        Span at=parse_span(input,state.at);
        if(enumeration){
            Text member=parse_identifier(input,state);if(state.error.kind!=0){break;}
            TypeRef payload=TypeRef(0u64);
            bool typed=false;
            if(take_word(input,state.at,"(")){
                payloads=true;
                if(!take_word(input,state.at,")")){
                    payload=parse_type(input,state);typed=true;
                    if(state.error.kind!=0||!expect_word(input,state,")")){break;}
                }
            }
            if(!typed){payload=publish_type(input,state,named_type(stored_word(input,state,"void")));}
            if(state.error.kind!=0||!expect_word(input,state,"=")){break;}
            ExprRef discriminant=parse_expression(input,state,0,0);
            if(state.error.kind!=0||!expect_word(input,state,";")){break;}
            temporary_push(state.error,variants,VariantNode(member,payload,discriminant,at));
        }else{
            TypeRef ty=parse_type(input,state);
            if(state.error.kind!=0){break;}
            Text field=parse_identifier(input,state);
            if(state.error.kind!=0||!expect_word(input,state,";")){break;}
            StorageIndex stored=put_field(input.storage,FieldNode(field,ty,at));parse_storage_error(state.error,stored.error);
            if(state.error.kind==0){temporary_push(state.error,fields,stored.offset);}
        }
    }
    if(state.error.kind==0){expect_word(input,state,";");}
    u64 end=parse_span(input,state.at-1u64).end;
    TemporaryValues published=temporary_values();u64 index=0u64;
    while(state.error.kind==0&&index<variants.count){
        VariantNode variant=temporary_at<VariantNode>(variants,index);
        if(payloads){
            StorageIndex stored=put_variant(input.storage,variant);parse_storage_error(state.error,stored.error);
            if(state.error.kind==0){temporary_push(state.error,published,stored.offset);}
        }else{
            Text member=joined_variant_name(input,state,name,variant.name);
            TypeRef result=publish_type(input,state,named_type(name));
            if(state.error.kind!=0){break;}
            StorageSlice arguments=indices(input.storage,address_of(variant.discriminant.offset),1u64);
            parse_storage_error(state.error,arguments.error);if(state.error.kind!=0){break;}
            ExprRef value=publish_expression(input,state,ExprNode(ExprKind::Call(CallExpr(name,arguments.value)),variant.span));
            if(state.error.kind!=0){break;}
            FunctionNode function=function_node(member,result,variant.span);function.kind=2;
            function.body=Optional<Slice>(1,generated_return(input,state,value,variant.span));
            if(state.error.kind!=0){break;}
            FunctionRef stored=publish_function(input,state,function);
            if(state.error.kind==0){temporary_push(state.error,functions,stored.offset);}
        }
        index+=1u64;
    }
    temporary_free(state.error,variants);
    Slice field_refs=finish_indices(input,state,fields);Slice variant_refs=finish_indices(input,state,published);
    if(state.error.kind!=0){return RecordRef(0u64);}
    StorageIndex stored=put_record(input.storage,RecordNode(name,parameters.types,parameters.values,stride,representation,
        variant_refs,field_refs,Span(span.start,end)));
    parse_storage_error(state.error,stored.error);return RecordRef(stored.offset);
}
Text joined_variant_name(read ParseInputs input,inout ParseState state,Text name,Text member) on cpu {
    u64 left=name.range.end-name.range.start;u64 right=member.range.end-member.range.start;u64 bytes=left+2u64+right;
    address memory=HeapAlloc(GetProcessHeap(),0,bytes);
    if(address_bits(memory)==0u64){parse_storage_error(state.error,StorageError(2,0));return empty_text();}
    u64 at=0u64;
    while(at<left){unsafe_position<u8>(unsafe_address(address_bits(memory)+at))=u8(stored_byte(input,name,at));at+=1u64;}
    unsafe_position<u8>(unsafe_address(address_bits(memory)+left))=u8(58);
    unsafe_position<u8>(unsafe_address(address_bits(memory)+left+1u64))=u8(58);
    at=0u64;
    while(at<right){unsafe_position<u8>(unsafe_address(address_bits(memory)+left+2u64+at))=u8(stored_byte(input,member,at));at+=1u64;}
    StorageText stored=text(input.storage,memory,bytes);parse_storage_error(state.error,stored.error);
    temporary_free(state.error,TemporaryValues(memory,0u64,1u64));return stored.value;
}
extern "C" ModuleRef parse_module(read ParseInputs input,inout ParseState state) on cpu {
    if(!expect_word(input,state,"module")){return ModuleRef(0u64);}
    Text name=qualified_text(input,state,46,false);
    if(state.error.kind!=0||!expect_word(input,state,";")){return ModuleRef(0u64);}
    TemporaryValues imports=temporary_values();TemporaryValues records=temporary_values();
    TemporaryValues type_sets=temporary_values();TemporaryValues functions=temporary_values();
    while(state.error.kind==0&&!word_is(input,state.at,"")){
        Span span=parse_span(input,state.at);
        if(take_word(input,state.at,"import")){
            Text value=qualified_text(input,state,46,false);
            if(state.error.kind!=0||!expect_word(input,state,";")){break;}
            temporary_push(state.error,imports,value);
        }else if(take_word(input,state.at,"const")){
            TypeRef result=parse_type(input,state);if(state.error.kind!=0){break;}
            Text member=parse_identifier(input,state);
            if(state.error.kind!=0||!expect_word(input,state,"=")){break;}
            ExprRef value=parse_expression(input,state,0,0);
            if(state.error.kind!=0||!expect_word(input,state,";")){break;}
            FunctionNode function=function_node(member,result,Span(span.start,parse_span(input,state.at-1u64).end));
            function.kind=2;function.body=Optional<Slice>(1,generated_return(input,state,value,span));
            if(state.error.kind!=0){break;}
            FunctionRef stored=publish_function(input,state,function);
            if(state.error.kind==0){temporary_push(state.error,functions,stored.offset);}
        }else if(take_word(input,state.at,"typeset")){
            Text member=parse_identifier(input,state);
            if(state.error.kind!=0||!expect_word(input,state,"{")){break;}
            TemporaryValues members=temporary_values();
            while(state.error.kind==0&&!take_word(input,state.at,"}")){
                TypeRef ty=parse_type(input,state);
                if(state.error.kind!=0||!expect_word(input,state,";")){break;}
                temporary_push(state.error,members,ty.offset);
            }
            if(state.error.kind==0){expect_word(input,state,";");}
            Slice member_refs=finish_indices(input,state,members);if(state.error.kind!=0){break;}
            StorageIndex stored=put_type_set(input.storage,TypeSetNode(member,member_refs,Span(span.start,parse_span(input,state.at-1u64).end)));
            parse_storage_error(state.error,stored.error);if(state.error.kind==0){temporary_push(state.error,type_sets,stored.offset);}
        }else if(word_is(input,state.at,"enum")||word_is(input,state.at,"struct")){
            bool enumeration=take_word(input,state.at,"enum");if(!enumeration){state.at+=1u64;}
            RecordRef stored=read_record(input,state,span,enumeration,functions);
            if(state.error.kind==0){temporary_push(state.error,records,stored.offset);}
        }else{
            FunctionRef stored=read_function(input,state,span);
            if(state.error.kind==0){temporary_push(state.error,functions,stored.offset);}
        }
    }
    Slice import_refs=finish_texts(input,state,imports);Slice record_refs=finish_indices(input,state,records);
    Slice set_refs=finish_indices(input,state,type_sets);Slice function_refs=finish_indices(input,state,functions);
    if(state.error.kind!=0){return ModuleRef(0u64);}
    StorageIndex stored=put_module(input.storage,ModuleNode(name,import_refs,record_refs,set_refs,function_refs));
    parse_storage_error(state.error,stored.error);return ModuleRef(stored.offset);
}
void finish_input(read ParseInputs input,inout ParseState state) on cpu {
    if(take_word(input,state.at,"")){return;}
    parse_fail(state.error,10,0,parse_span(input,state.at));
    state.error.expected=empty_text();
}
extern "C" ParsedEntry parse_entry(read ParseInputs input,inout ParseState state) on cpu {
    Text name=parse_type_name(input,state);Optional<Slice> arguments=Optional<Slice>(0,Slice(0u64,0u64));
    if(state.error.kind==0&&word_is(input,state.at,"<")){arguments=Optional<Slice>(1,generic_arguments(input,state));}
    if(state.error.kind==0){finish_input(input,state);}
    return ParsedEntry(name,arguments);
}
extern "C" TypeRef parse_type_query(read ParseInputs input,inout ParseState state) on cpu {
    TypeRef value=parse_type(input,state);
    if(state.error.kind==0){finish_input(input,state);}return value;
}
