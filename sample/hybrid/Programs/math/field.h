module math.field;

// The teaching algorithms use f32 energy and four f32 dimensions.
typeset FieldValue { f32; float2; float3; float4; };

T central_acceleration<T: FieldValue, const Power: u32 in {1, 2, 3}>(T position, f32 strength);
