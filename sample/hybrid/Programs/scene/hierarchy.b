module scene.hierarchy;
float3x4 compose_affine(float3x4 parent,float3x4 child) on compute {
    float3x3 linear=float3x3(parent.r0.xyz,parent.r1.xyz,parent.r2.xyz);
    float3x4 product=linear*child;
    return float3x4(product.r0+float4(0.0,0.0,0.0,parent.r0.w),
                     product.r1+float4(0.0,0.0,0.0,parent.r1.w),
                     product.r2+float4(0.0,0.0,0.0,parent.r2.w));
}
program resolve(read dataset<BLHierarchyNode> nodes,write dataset<BLWorldTransform> worlds) on compute {
    parallel(world in worlds) {
        BLHierarchyNode node=unsafe_read(nodes,index());
        float3x4 matrix=node.local;
        if(node.active){
            while(node.has_parent){
                node=unsafe_read(nodes,node.parent);
                matrix=compose_affine(node.local,matrix);
            }
        }
        world.matrix=matrix;
    }
}
program descendants(read dataset<BLHierarchyNode> nodes,write dataset<BLSubtree> members,u32 root) on compute {
    parallel(member in members){
        BLHierarchyNode node=unsafe_read(nodes,index());
        member.member=false;
        if(node.active){
            u32 current=index();
            while(true){
                if(current==root){member.member=true;break;}
                node=unsafe_read(nodes,current);
                if(!node.has_parent){break;}
                current=node.parent;
            }
        }
    }
}
