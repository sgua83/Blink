module scene.components;
void cameras(read dataset<BLWorldTransform> worlds,read dataset<BLAttachedCamera> locals,
                write dataset<BLSceneCamera> output) on compute {
    parallel(camera in output) {
        BLAttachedCamera source=unsafe_read(locals,index());
        float3x4 world=unsafe_read(worlds,source.node).matrix;
        BLSceneCamera value=source.camera;
        value.eye=world*float4(value.eye,1.0);
        value.target=world*float4(value.target,1.0);
        value.up=world*float4(value.up,0.0);
        camera=value;
    }
}
void lights(read dataset<BLWorldTransform> worlds,read dataset<BLAttachedLight> locals,
               write dataset<BLLight> output) on compute {
    parallel(light in output) {
        BLAttachedLight source=unsafe_read(locals,index());
        float3x4 world=unsafe_read(worlds,source.node).matrix;
        BLLight value=source.light;
        if(value.kind!=0){value.position=world*float4(value.position,1.0);}
        if(value.kind==0||value.kind==2){value.direction=world*float4(value.direction,0.0);}
        if(value.kind==3){
            value.edge_u=world*float4(value.edge_u,0.0);
            value.edge_v=world*float4(value.edge_v,0.0);
        }
        light=value;
    }
}
void evaluate(read dataset<BLHierarchyNode> nodes,inout dataset<BLWorldTransform> worlds,
                 read dataset<BLAttachedCamera> local_cameras,write dataset<BLSceneCamera> world_cameras,
                 read dataset<BLAttachedLight> local_lights,write dataset<BLLight> world_lights) on compute {
    resolve(nodes,worlds);
    cameras(worlds,local_cameras,world_cameras);
    lights(worlds,local_lights,world_lights);
}
