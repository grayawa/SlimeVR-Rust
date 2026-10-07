#!/usr/bin/env python3
"""Run actual ktmath/filter/reset/Bone/TransformNode and extracted HumanSkeleton methods.

External tracker/server callbacks are isolated. Arms, fingers, constraints,
LegTweaks, StayAligned, Localizer, derived velocity and AutoBone frame shuffling are included.
AutoBone training and the full Java service are not executed. Inputs are synthetic.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import urllib.request

HERE=Path(__file__).resolve().parent
ROOT=HERE.parent.parent
SOURCE=ROOT/'server/core/src/main/java'

def quat(axis=(0,1,0),angle=0):
    s=math.sin(angle/2)
    return dict(w=math.cos(angle/2),x=axis[0]*s,y=axis[1]*s,z=axis[2]*s)

def vector(x=0,y=0,z=0):return dict(x=x,y=y,z=z)

def skeleton_config(**kwargs):
    c=dict(head_shift=.1,neck_length=.1,upper_chest_length=.16,chest_length=.16,waist_length=.20,hip_length=.04,
           hips_width=.26,upper_leg_length=.42,lower_leg_length=.50,foot_length=.05,foot_shift=-.05,
           chest_offset=0.,hip_offset=0.,skeleton_offset=0.,shoulders_distance=.08,shoulders_width=.35,upper_arm_length=.26,lower_arm_length=.26,hand_y=.035,hand_z=.13,elbow_offset=0.,force_arms_from_hmd=True,enforce_constraints=False,extended_spine=True,extended_pelvis=True,extended_knee=True,
           waist_from_chest_hip=.30,waist_from_chest_legs=.30,hip_from_chest_legs=.50,hip_from_waist_legs=.40,
           hip_legs=.25,knee_tracker_ankle=.85,knee_ankle=0.)
    c.update(kwargs);return c

def offsets(c):
    out=[('head',vector(z=c['head_shift'])),('neck',vector(y=-c['neck_length']))]
    for name in ('upper_chest','chest','waist','hip'):out.append((name,vector(y=-c[name+'_length'])))
    for side,sign in [('left',-1),('right',1)]:
        out.extend([(side+'_hip',vector(x=sign*c['hips_width']/2)),
                    (side+'_upper_leg',vector(y=-c['upper_leg_length'])),
                    (side+'_lower_leg',vector(y=-c['lower_leg_length'],z=-c['foot_shift'])),
                    (side+'_foot',vector(z=-c['foot_length']))])
    out.extend([('head_tracker',vector()),('chest_tracker',vector(y=-c['chest_length']-c['chest_offset'],z=-c['skeleton_offset'])),('hip_tracker',vector(y=-c['hip_offset'],z=-c['skeleton_offset']))])
    for name in ('left_knee','right_knee','left_foot','right_foot'):out.append((name+'_tracker',vector(z=-c['skeleton_offset'])))
    for side,sign in [('left',-1),('right',1)]:
        out.extend([(side+'_upper_shoulder',vector()),(side+'_shoulder',vector(sign*c['shoulders_width']/2,-c['shoulders_distance'])),(side+'_upper_arm',vector(y=-c['upper_arm_length'])),(side+'_lower_arm',vector(y=-c['lower_arm_length'])),(side+'_hand',vector(y=-c['hand_y'],z=-c['hand_z'])),(side+'_elbow_tracker',vector(y=-c['elbow_offset'])),(side+'_hand_tracker',vector())])
        for finger,scale in [('thumb',.2),('index',.25),('middle',.3),('ring',.28),('little',.2)]:
            for segment,ratio in zip(['metacarpal','proximal','distal'] if finger=='thumb' else ['proximal','intermediate','distal'],[2.3,1.3,1.0]):
                length=c['hand_y']*scale*3/4.6*ratio
                out.append((f'{side}_{finger}_{segment}',vector(y=-length,z=-length*.5 if finger=='thumb' else 0)))
    return [dict(name=name,offset=offset) for name,offset in out]

def cases():
    out=[]
    for i,(a,b,t) in enumerate([
        (quat(),quat(angle=1.3),0),(quat(),quat(angle=1.3),1),
        (quat((1,0,0),.6),quat((0,0,1),1.1),.2),
        (quat((1,0,0),.6),quat((0,0,1),1.1),.7),
        (quat(angle=2.9),quat(angle=-2.9),1.4),
        (quat((0,0,1),math.pi/2),quat((1,0,0),.4),.35),
        (dict(w=.7,x=.2,y=-.4,z=.1),dict(w=.8,x=-.1,y=.3,z=.2),-.2),
        (dict(w=-1.,x=0.,y=0.,z=0.),quat(),.5),
    ]):out.append(dict(name=f'math_{i}',kind='math',a=a,b=b,t=t,vector=vector(1.2,-.7,.3)))
    for mode in ['none','smoothing','prediction']:
        for amount in ([.2,1.7] if mode=='smoothing' else [.2]):
            steps=[]
            for i in range(14):
                q=quat((1,0,0),.15*i) if i<7 else quat((0,0,1),-.13*i)
                if i%3==2:q={k:-v for k,v in q.items()}
                steps.extend([dict(op='sample',q=q),dict(op='tick',dt=.001),dict(op='tick',dt=.006)])
            steps.extend([dict(op='reset',q=quat(angle=.4),reference=quat()),dict(op='tick',dt=.003),dict(op='tick',dt=.15)])
            out.append(dict(name=f'filter_{mode}_{amount}',kind='filter',mode=mode,amount=amount,initial=quat(),steps=steps))
    for body in ['chest','hip','left_upper_leg','right_lower_leg']:
        for smooth in [0.,.25]:
            steps=[dict(op='sample',q=quat((1,0,0),.7)),dict(op='full',reference=quat(angle=.5)),
                   dict(op='sample',q=quat((0,0,1),.9)),dict(op='yaw',reference=quat(angle=-.8)),
                   dict(op='tick',dt=.025),dict(op='tick',dt=.03),dict(op='tick',dt=.3),
                   dict(op='sample',q=quat((1,0,0),-.8)),dict(op='mounting',reference=quat(angle=.2)),
                   dict(op='sample',q=quat((0,0,1),-.5)),dict(op='full',reference=quat(angle=.4))]
            out.append(dict(name=f'calibration_{body}_{smooth}',kind='calibration',body=body,mounting=quat(angle=math.pi),smooth_seconds=smooth,steps=steps))
    for body in ['head','chest','left_upper_arm','right_upper_arm']:
        for mode in ['back','tpose_down']:
            steps=[dict(op='sample',q=quat((1,0,0),.7)),dict(op='full',reference=quat(angle=.5)),dict(op='sample',q=quat((0,0,1),.9)),dict(op='yaw',reference=quat(angle=-.8)),dict(op='tick',dt=.025),dict(op='tick',dt=.3),dict(op='full',reference=quat(angle=.2))]
            out.append(dict(name=f'computed_{body}_{mode}',kind='calibration',computed=True,body=body,arms_mode=mode,mounting=quat(),smooth_seconds=.25,steps=steps))
    for angle in [math.pi/2,-math.pi/2,math.pi/2-1e-5]:
        out.append(dict(name=f'math_yxz_singular_{angle}',kind='math',a=quat((1,0,0),angle),b=quat(angle=.3),t=.5,vector=vector(1,2,3)))
    for body in ['chest','left_shoulder','right_shoulder','left_thumb_metacarpal','right_thumb_proximal','left_thumb_distal','left_index_proximal','right_index_intermediate','right_index_distal']:
        steps=[dict(op='resistance',value=v) for v in [100.,140.,200.,120.]]+[dict(op='min')]+[dict(op='resistance',value=v) for v in [180.,80.]]+[dict(op='max')]+[dict(op='resistance',value=v) for v in [70.,100.,130.]]+[dict(op='min'),dict(op='resistance',value=200.),dict(op='max'),dict(op='angle',value=.4)]
        out.append(dict(name=f'flex_{body}',kind='flex',body=body,steps=steps))
    layouts=[
        ('standing_six',dict(chest=quat(),hip=quat(),left_upper_leg=quat(),right_upper_leg=quat(),left_lower_leg=quat(),right_lower_leg=quat())),
        ('bent_six',dict(chest=quat((1,0,0),.35),hip=quat((0,0,1),-.2),left_upper_leg=quat((1,0,0),-.6),right_upper_leg=quat((1,0,0),-.4),left_lower_leg=quat((1,0,0),.8),right_lower_leg=quat((1,0,0),.7))),
        ('missing_hip',dict(chest=quat((1,0,0),.3),left_upper_leg=quat((0,0,1),-.3),right_upper_leg=quat((0,0,1),.2))),
        ('waist_and_legs',dict(waist=quat((1,0,0),.2),left_upper_leg=quat((1,0,0),-.7),right_upper_leg=quat((1,0,0),-.4))),
        ('single_thigh',dict(chest=quat(angle=.6),left_upper_leg=quat((1,0,0),-.5))),
        ('hmd_only',{}),('no_inputs',{}),
    ]
    for name,inputs in layouts:
        for anchored in [True,False]:
            c=skeleton_config(skeleton_offset=.035,chest_offset=.025,hip_offset=.01)
            out.append(dict(name=f'skeleton_{name}_{anchored}',kind='skeleton',inputs=inputs,head=dict(rotation=quat(angle=.4),position=vector(.2,1.7,-.1)) if anchored else None,config=c,offsets=offsets(c)))
    c=skeleton_config(extended_spine=False,extended_pelvis=False,extended_knee=False,foot_shift=0.,knee_ankle=.4)
    out.append(dict(name='skeleton_extensions_disabled',kind='skeleton',inputs=layouts[1][1],head=dict(rotation=quat(),position=vector(y=1.7)),config=c,offsets=offsets(c)))
    frames=[]
    c=skeleton_config()
    for heading in [.4,-.6,1.1]:
        frames.append(dict(inputs={},head=dict(rotation=quat(angle=heading),position=vector(y=1.7)),config=c,offsets=offsets(c)))
    frames.append(dict(inputs={},head=None,config=c,offsets=offsets(c)))
    out.append(dict(name='skeleton_previous_neck_frame_and_missing_all',kind='skeleton_sequence',frames=frames))
    for controller in [False,True]:
        for constraints in [False,True]:
            c=skeleton_config(force_arms_from_hmd=not controller,enforce_constraints=constraints)
            inputs=dict(layouts[1][1],left_upper_arm=quat((0,0,1),-.8),right_lower_arm=quat((1,0,0),-.6),left_hand=quat(angle=.2),right_hand=quat(angle=-.4),left_index_proximal=quat((1,0,0),-.2),right_thumb_distal=quat((1,0,0),-.8))
            positions={'left_hand':vector(-.3,1.2,-.4),'right_hand':vector(.4,1.1,-.3)} if controller else {}
            out.append(dict(name=f'arms_{controller}_{constraints}',kind='skeleton',config=c,inputs=inputs,positions=positions,head=dict(rotation=quat(),position=vector(y=1.7)),offsets=offsets(c)))
    for mode in ['back','forward','tpose_up','tpose_down']:
        for body in ['left_upper_arm','right_lower_arm','left_hand','right_thumb_distal']:
            base=next(c for c in out if c['kind']=='calibration').copy()
            base.update(name=f'arm_reset_{body}_{mode}',body=body,arms_mode=mode)
            out.append(base)
    for mode in ['clip','skating','rotation','all']:
        legs=dict(enabled=True,floor_clip=mode in ['clip','all'],skating=mode in ['skating','all'],toe_snap=mode in ['rotation','all'],foot_plant=mode in ['rotation','all'],always_use_floor_clip=False,correction_strength=.3)
        frames=[];c=skeleton_config()
        for i in range(90):
            inputs=dict(layouts[0][1]);inputs.update(left_upper_leg=quat((1,0,0),-.04*max(i-20,0)),left_lower_leg=quat((1,0,0),.02*max(i-20,0)))
            if i>=65:inputs['right_lower_leg']=quat((0,0,1),.4)
            frame=dict(at_ms=(i+1)*4,config=c,offsets=offsets(c),inputs=inputs,head=dict(rotation=quat(),position=vector(.02*math.sin(i*.07),1.58-.002*i,0)),accelerations={})
            if i in [25,26,60]:frame['accelerations']={'left_lower_leg':vector(y=2.0)}
            frames.append(frame)
        out.append(dict(name='legs_'+mode,kind='legs',legs=legs,reset_floor=True,frames=frames))
    # Exercise one planted leg while the other is raised, with the production
    # constraint toggle and a moving HMD anchor (the original fixtures disabled constraints).
    for side in ['left', 'right']:
        for constrained in [False, True]:
            c=skeleton_config(enforce_constraints=constrained)
            frames=[]
            for i in range(180):
                lift=max(0.,min((i-30)/60.,1.))
                inputs=dict(layouts[0][1])
                inputs[side+'_upper_leg']=quat((1,0,0),-lift*1.3)
                inputs[side+'_lower_leg']=quat((1,0,0),lift*.4)
                inputs['hip']=quat((0,0,1),.04*lift)
                frames.append(dict(at_ms=(i+1)*4,config=c,offsets=offsets(c),inputs=inputs,
                    head=dict(rotation=quat(),position=vector(.025*math.sin(i*.015),1.7,0)),accelerations={}))
            out.append(dict(name=f'legs_single_lift_{side}_constraints_{constrained}',kind='legs',
                reset_floor=True,legs=dict(enabled=True,floor_clip=True,skating=True,toe_snap=False,
                foot_plant=True,always_use_floor_clip=False,correction_strength=.3),frames=frames))
    frames=[]
    for i in range(600):
        rotations=dict(chest=quat(),hip=quat(),left_upper_leg=quat(angle=.4),right_upper_leg=quat(angle=-.3),left_lower_leg=quat(angle=.2),right_lower_leg=quat(angle=-.1))
        if i>60:rotations['left_upper_leg']=quat((1,0,0),.07*math.sin(i*.2))
        if i>360:rotations['hip']=quat((0,0,1),.2)
        frames.append(dict(at_ms=i*4,rotations=rotations))
    out.append(dict(kind='alignment',name='alignment_moving_rest_recently_rest',frames=frames))
    c=skeleton_config();frames=[]
    for i in range(240):
        inputs=dict(layouts[0][1])
        if i>140:inputs.update(left_upper_leg=quat((1,0,0),-.25*math.sin(i*.06)),left_lower_leg=quat((1,0,0),.3*math.sin(i*.06)))
        frames.append(dict(at_ms=(i+1)*4,config=c,offsets=offsets(c),inputs=inputs,head=None,accelerations={}))
    out.append(dict(kind='legs',name='localizer_standing_walking_no_hmd',localizer=True,reset_floor=False,legs=dict(enabled=True,floor_clip=True,skating=True,toe_snap=False,foot_plant=True,correction_strength=.3),frames=frames))
    for count in [1,16,31,48,64]:out.append(dict(kind='frame_orders',name=f'autobone_frame_orders_{count}',count=count,epochs=4,seed=4))
    for pitch,roll in [(.6,0.),(.4,.3),(-.7,-.5)]:
        # Compose with ktmath multiplication in the oracle, using these explicit input quaternions.
        def multiply(a,b):
            return dict(w=a['w']*b['w']-a['x']*b['x']-a['y']*b['y']-a['z']*b['z'],
                x=a['w']*b['x']+a['x']*b['w']+a['y']*b['z']-a['z']*b['y'],
                y=a['w']*b['y']-a['x']*b['z']+a['y']*b['w']+a['z']*b['x'],
                z=a['w']*b['z']+a['x']*b['y']-a['y']*b['x']+a['z']*b['w'])
        rotation=lambda yaw,p: multiply(multiply(quat(angle=yaw),quat((0,0,1),roll)),quat((1,0,0),p))
        out.append(dict(kind='hmd_calibration',name=f'hmd_pitch_{pitch}_{roll}',steps=[
            dict(rotation=rotation(.8,pitch)),dict(full=True,enabled=False),dict(full=True,enabled=True),
            dict(rotation=rotation(-.4,pitch+.2)),dict(full=True,enabled=True),dict(full=True,enabled=False)]))
    for posture,pitch in [('standing',0.),('sitting',-math.pi/2),('flat',-math.pi/2)]:
        frames=[]
        for i in range(360):
            turns=.1*math.sin(i*.07)
            rotations=dict(chest=quat((1,0,0),pitch if posture=='flat' else 0.),hip=quat((1,0,0),pitch if posture=='flat' else 0.),
                left_upper_leg=multiply(quat(angle=.4+turns),quat((1,0,0),pitch)),right_upper_leg=multiply(quat(angle=-.3-turns),quat((1,0,0),pitch)),
                left_lower_leg=quat((1,0,0),pitch if posture=='flat' else 0.),right_lower_leg=quat((1,0,0),pitch if posture=='flat' else 0.),
                left_foot=quat(angle=.3),right_foot=quat(angle=-.2))
            frames.append(dict(at_ms=i*4,rotations=rotations))
        out.append(dict(kind='alignment',name='alignment_nonzero_relaxed_'+posture,frames=frames,
            relaxed=dict(upper_leg_degrees=12.,lower_leg_degrees=6.,foot_degrees=4.)))
    velocity_steps = [
        dict(at_us=0, position=vector()),
        dict(at_us=99, position=vector(.01, -.02, .03)),
        dict(at_us=199, position=vector(.02, -.04, .06)),
        dict(at_us=250199, position=vector(.12, -.14, .36)),
        dict(at_us=500200, position=vector(.2, -.1, .4)),
        dict(at_us=504200, position=vector(.21, -.09, .38)),
        dict(at_us=504200, position=vector(.4, .3, -.2)),
        dict(at_us=508200, position=vector(.41, .32, -.22)),
    ]
    out.append(dict(kind='velocity', name='derived_velocity_interval_boundaries_and_recovery', steps=velocity_steps))
    out.append(dict(kind='velocity', name='derived_velocity_disable_position_loss_and_reset', steps=[
        dict(at_us=0, position=vector()), dict(at_us=4000, position=vector(.01, .02, -.03)),
        dict(at_us=8000, position=vector(1, 2, 3), enabled=False),
        dict(at_us=12000, position=vector(2, 3, 4)), dict(at_us=16000, position=vector(2.01, 3.02, 4.03)),
        dict(at_us=20000, position=None), dict(at_us=24000, position=vector()),
        dict(at_us=28000, position=vector(.1, .2, .3)),
        dict(at_us=32000, position=vector(10, 20, 30), reset=True),
        dict(at_us=36000, position=vector(10.01, 20.02, 30.03)),
    ]))
    return out

def extract(text,name):
    start=re.search(r'\t(?:private )?fun '+name+r'\(',text).start()
    opening=text.index('{',start);depth=1;end=opening+1
    while depth:
        if text[end]=='{':depth+=1
        if text[end]=='}':depth-=1
        end+=1
    return text[start:end]

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cache',type=Path,default=Path(tempfile.gettempdir())/'slimevr-udp-oracle')
    parser.add_argument('--javac-java-modules',type=Path)
    parser.add_argument('--output',type=Path,default=HERE.parent/'crates/slimevr-core/tests/fixtures/core-golden.json')
    args=parser.parse_args()
    spec=importlib.util.spec_from_file_location('udp_generator',HERE/'generate-udp-golden.py');mod=importlib.util.module_from_spec(spec);spec.loader.exec_module(mod)
    artifacts=mod.ARTIFACTS+[("com.fasterxml.jackson.core","jackson-databind","2.21.0"),("com.fasterxml.jackson.core","jackson-core","2.21.0"),("com.fasterxml.jackson.core","jackson-annotations","2.21")]
    lib=args.cache/'lib';lib.mkdir(parents=True,exist_ok=True)
    def fetch(item):
        g,a,version=item;p=lib/f'{a}-{version}.jar'
        if not p.exists():urllib.request.urlretrieve(f'https://repo.maven.apache.org/maven2/{g.replace(".","/")}/{a}/{version}/{p.name}',p)
        return p
    with ThreadPoolExecutor(max_workers=4) as pool:list(pool.map(fetch,artifacts))
    classes=args.cache/'core-classes';classes.mkdir(exist_ok=True)
    java_sources=[SOURCE/'dev/slimevr/filtering/CircularArrayList.java',SOURCE/'dev/slimevr/tracking/processor/BoneType.java',SOURCE/'io/eiren/util/ann/ThreadSafe.java',SOURCE/'dev/slimevr/tracking/processor/config/SkeletonConfigToggles.java',ROOT/'solarxr-protocol/protocol/java/src/solarxr_protocol/datatypes/BodyPart.java']
    if not shutil.which('javac') and not args.javac_java_modules:
        parser.error('JDK javac is required; alternatively specify --javac-java-modules')
    javac=[shutil.which('javac')] if shutil.which('javac') else ['java','--module-path',str(args.javac_java_modules),'--add-modules','jdk.compiler','-m','jdk.compiler/com.sun.tools.javac.Main']
    subprocess.run([*javac,'-d',str(classes),*map(str,java_sources)],check=True)
    relative=['dev/slimevr/filtering/QuaternionMovingAverage.kt','dev/slimevr/filtering/TrackerFilters.kt','dev/slimevr/tracking/trackers/TrackerFlexHandler.kt','dev/slimevr/tracking/trackers/TrackerResetsHandler.kt','dev/slimevr/tracking/trackers/TrackerPosition.kt','dev/slimevr/tracking/trackers/TrackerRole.kt','dev/slimevr/tracking/trackers/udp/FirmwareConstants.kt','dev/slimevr/tracking/processor/Bone.kt','dev/slimevr/tracking/processor/TransformNode.kt','dev/slimevr/tracking/processor/Constraint.kt','dev/slimevr/util/InterpolationHandler.kt','dev/slimevr/config/LegTweaksConfig.kt','dev/slimevr/tracking/processor/skeleton/LegTweaks.kt','dev/slimevr/tracking/processor/skeleton/Localizer.kt','io/eiren/math/FloatMath.kt','com/jme3/math/FastMath.kt']
    sources=[SOURCE/p for p in relative]+sorted((SOURCE/'io/github/axisangles/ktmath').glob('*.kt'))
    sources+=[SOURCE/'dev/slimevr/math'/name for name in ['Angle.kt','AngleAverage.kt','AngleErrors.kt']]
    sources+=[SOURCE/'dev/slimevr/config'/name for name in ['StayAlignedConfig.kt','StayAlignedRelaxedPoseConfig.kt']]
    sources+=sorted(p for p in (SOURCE/'dev/slimevr/tracking/processor/stayaligned').rglob('*.kt') if p.name!='RestDetector.kt')
    rest_source=SOURCE/'dev/slimevr/tracking/processor/stayaligned/trackers/RestDetector.kt'
    rest_generated=args.cache/'RestDetector.kt';rest_generated.write_text(rest_source.read_text().replace('TimeSource.Monotonic.markNow()','OracleTimeSource.markNow()').replace('package dev.slimevr.tracking.processor.stayaligned.trackers','package dev.slimevr.tracking.processor.stayaligned.trackers\nimport OracleTimeSource'));sources.append(rest_generated)
    sources+=sorted(p for p in (HERE/'core-reference').glob('*.kt') if not p.name.endswith('.template.kt') and p.name!='LegSkeletonStub.kt')
    buffer_source=SOURCE/'dev/slimevr/tracking/processor/skeleton/LegTweaksBuffer.kt'
    generated_buffer=args.cache/'LegTweaksBuffer.kt';generated_buffer.write_text(buffer_source.read_text().replace('System.nanoTime()','OracleClock.nanos').replace('package dev.slimevr.tracking.processor.skeleton','package dev.slimevr.tracking.processor.skeleton\nimport OracleClock'))
    sources.append(generated_buffer)
    stub=(HERE/'core-reference/LegSkeletonStub.kt').read_text()
    bone_names=['head','chest','waist','hip','upper_chest','hip_tracker','left_foot','right_foot','left_lower_leg','right_lower_leg','left_upper_leg','right_upper_leg','left_lower_arm','right_lower_arm','left_upper_arm','right_upper_arm']
    camel=lambda name:re.sub(r'_([a-z])',lambda m:m[1].upper(),name)
    stub=stub.replace('// LEG_BONES','\n'.join(f'val {camel(name)}Bone get()=bones.getValue(BoneType.{name.upper()})' for name in bone_names))
    tracker_names=['neck','upper_chest','left_shoulder','right_shoulder','left_hand','right_hand','head','waist','hip','chest','left_foot','right_foot','left_lower_leg','right_lower_leg','left_upper_leg','right_upper_leg','left_lower_arm','right_lower_arm','left_upper_arm','right_upper_arm']
    stub=stub.replace('// LEG_TRACKERS','\n'.join(f'val {camel(name)}Tracker get()=trackers[TrackerPosition.{name.upper()}]' for name in tracker_names))
    computed_names=['head','chest','hip','left_elbow','right_elbow','left_hand','right_hand','left_knee','right_knee','left_foot','right_foot']
    stub=stub.replace('// COMPUTED_TRACKERS','\n'.join(f'val computed{camel(name)[0].upper()+camel(name)[1:]}Tracker get()=computed["{name}"]' for name in computed_names))
    generated_stub=args.cache/'LegSkeletonStub.kt';generated_stub.write_text(stub);sources.append(generated_stub)
    human=SOURCE/'dev/slimevr/tracking/processor/skeleton/HumanSkeleton.kt'
    extracted=['assembleSkeleton','assembleSkeletonArms','updateArmTransforms','updateFingerTransforms','updateHeadTransforms','updateSpineTransforms','updateLegTransforms','extendedKneeYawRoll','extendedPelvisYawRoll','updateNodeOffset']
    methods='\n\n'.join(extract(human.read_text(),name) for name in extracted)
    names=[p['name'] for p in offsets(skeleton_config())]
    def camel(name):return re.sub(r'_([a-z])',lambda m:m[1].upper(),name)
    positions=[n for n in names if not n.endswith('_tracker') and n not in ['left_hip','right_hip','left_upper_shoulder','right_upper_shoulder']]
    template=(HERE/'core-reference/SkeletonReference.template.kt').read_text()
    declarations=re.findall(r'val (\w+)Bone = (Bone\(BoneType\.\w+, Constraint\([^\n]+)',human.read_text())
    template=template.replace('// BONE_MAP',',\n'.join(re.search(r'BoneType\.\w+',expression)[0]+' to '+expression for _,expression in declarations))
    arm_names=[name for name in names if any(x in name for x in ['shoulder','arm','hand','elbow','thumb','index','middle','ring','little'])]
    template=template.replace('// ALL_ARM_BONES',','.join(camel(n)+'Bone' for n in arm_names))
    template=template.replace('// RUN_ARMS', '\n'.join(f'updateArmTransforms(isTracking{side.capitalize()}ArmFromController,{side}UpperShoulderBone,{side}ShoulderBone,{side}UpperArmBone,{side}ElbowTrackerBone,{side}LowerArmBone,{side}HandBone,{side}HandTrackerBone,{side}ShoulderTracker,{side}UpperArmTracker,{side}LowerArmTracker,{side}HandTracker)' for side in ['left','right']))
    template=template.replace('// RUN_FINGERS','\n'.join(f'updateFingerTransforms({side}HandTrackerBone.getGlobalRotation(),'+','.join(camel(f'{side}_{finger}_{seg}')+'Bone' for seg in (['metacarpal','proximal','distal'] if finger=='thumb' else ['proximal','intermediate','distal']))+','+','.join(camel(f'{side}_{finger}_{seg}')+'Tracker' for seg in (['metacarpal','proximal','distal'] if finger=='thumb' else ['proximal','intermediate','distal']))+')' for side in ['left','right'] for finger in ['thumb','index','middle','ring','little']))
    template=template.replace('// BONE_FIELDS' ,'\n'.join(f'private val {camel(name)}Bone get()=getBone(BoneType.{name.upper()})' for name in names))
    template=template.replace('// TRACKER_FIELDS','\n'.join(f'private var {camel(name)}Tracker:Tracker?=null' for name in positions))
    template=template.replace('// EXTRACTED_METHODS',methods)
    template=template.replace('// SET_TRACKERS','\n'.join(f'{camel(name)}Tracker=n["inputs"]["{name}"]?.let {{Tracker(TrackerPosition.{name.upper()}).apply {{directRotation=quaternion(it)}}}}' for name in positions))
    ratios={'waistFromChestHipAveraging':'waist_from_chest_hip','waistFromChestLegsAveraging':'waist_from_chest_legs','hipFromChestLegsAveraging':'hip_from_chest_legs','hipFromWaistLegsAveraging':'hip_from_waist_legs','hipLegsAveraging':'hip_legs','kneeTrackerAnkleAveraging':'knee_tracker_ankle','kneeAnkleAveraging':'knee_ankle'}
    template=template.replace('// SET_RATIOS','\n'.join(f'{name}=n["config"]["{key}"].floatValue()' for name,key in ratios.items()))
    generated=args.cache/'SkeletonReference.kt';generated.write_text(template)
    tracker_source=SOURCE/'dev/slimevr/tracking/trackers/Tracker.kt'
    velocity_method=extract(tracker_source.read_text(), 'updateDerivedVelocity').replace('TimeSource.Monotonic.markNow()', 'clock.markNow()')
    velocity_text="""import com.fasterxml.jackson.databind.JsonNode
import io.github.axisangles.ktmath.Vector3
import kotlin.time.*
import kotlin.time.Duration.Companion.microseconds
import kotlin.time.Duration.Companion.milliseconds
@OptIn(ExperimentalTime::class)
class VelocityReference {
    private val clock=TestTimeSource()
    private var allowVelocity=false
    private var hasPosition=false
    private var position=Vector3.NULL
    private var _velocity=Vector3.NULL
    private class VelocityState { var prevMark:ComparableTimeMark?=null;var prevPos=Vector3.NULL }
    private var velocityState=VelocityState()
// METHOD
    fun run(n:JsonNode):Any {
        var at=0L
        return n[\"steps\"].map { step ->
            val next=step[\"at_us\"].longValue();clock+=(next-at).microseconds;at=next
            if(step.path(\"reset\").asBoolean(false)) { velocityState=VelocityState();_velocity=Vector3.NULL }
            allowVelocity=step.path(\"enabled\").asBoolean(true)
            hasPosition=!step[\"position\"].isNull
            if(hasPosition)position=vector(step[\"position\"])
            updateDerivedVelocity()
            if(_velocity==Vector3.NULL)null else v(_velocity)
        }
    }
}
""".replace('// METHOD', velocity_method)
    velocity_generated=args.cache/'VelocityReference.kt';velocity_generated.write_text(velocity_text);sources.append(velocity_generated)
    iterator=SOURCE/'dev/slimevr/autobone/PoseFrameIterator.kt'
    orders=extract(iterator.read_text(),'randomIndices')
    orders_text='import kotlin.random.Random\nobject FrameOrdersReference {\n'+orders+'\nfun run(count:Int,epochs:Int,seed:Long):Any {val random=Random(seed);return (0 until epochs).map{randomIndices(count,random).toList()}}\n}'
    orders_generated=args.cache/'FrameOrdersReference.kt';orders_generated.write_text(orders_text);sources.append(orders_generated)
    cp=os.pathsep.join(map(str,[classes,*sorted(lib.glob('*.jar'))]))
    subprocess.run(['java','-cp',cp,'org.jetbrains.kotlin.cli.jvm.K2JVMCompiler','-Xvalue-classes','-no-stdlib','-no-reflect','-jvm-target','17','-classpath',cp,'-d',str(classes),*map(str,sources),str(generated)],check=True)
    selected=cases()
    result=subprocess.run(['java','-cp',cp,'CoreOracleKt'],input=''.join(json.dumps(c)+'\n' for c in selected),text=True,capture_output=True,check=True)
    expected=[json.loads(line) for line in result.stdout.splitlines()];assert len(expected)==len(selected)
    hashes={str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in [*[p for p in sources if p.is_relative_to(ROOT)],*java_sources,human,buffer_source,rest_source,iterator,tracker_source,HERE/'core-reference/LegSkeletonStub.kt',HERE/'core-reference/SkeletonReference.template.kt']}
    hashes['generated/VelocityReference.explicit-clock.kt']=hashlib.sha256(velocity_text.encode()).hexdigest()
    hashes['generated/FrameOrdersReference.kt']=hashlib.sha256(orders_text.encode()).hexdigest()
    hashes['generated/RestDetector.explicit-clock.kt']=hashlib.sha256(rest_generated.read_bytes()).hexdigest()
    hashes['generated/LegTweaksBuffer.explicit-clock.kt']=hashlib.sha256(generated_buffer.read_bytes()).hexdigest()
    hashes['generated/LegSkeletonStub.kt']=hashlib.sha256(stub.encode()).hexdigest()
    fixture=dict(reference_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),scope=__doc__,source_sha256=hashes,extracted_methods=extracted,extracted_source_sha256=hashlib.sha256(template.encode()).hexdigest(),cases=[dict(**c,expected=e) for c,e in zip(selected,expected)])
    args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(fixture,indent=2)+'\n')
    print(f'Generated {len(selected)} algorithm cases: {args.output}')

if __name__=='__main__':main()
