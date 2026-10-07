import com.fasterxml.jackson.databind.ObjectMapper
import com.fasterxml.jackson.databind.node.ObjectNode
import dev.slimevr.autobone.*
import dev.slimevr.autobone.errors.AutoBoneException
import dev.slimevr.config.*
import dev.slimevr.poseframeformat.PoseFrames
import dev.slimevr.poseframeformat.trackerdata.*
import dev.slimevr.tracking.processor.HumanPoseManager
import dev.slimevr.tracking.processor.OFFSET_FIELDS
import dev.slimevr.tracking.trackers.*
private val mapper=ObjectMapper()
private val fields=mapOf("epochs" to "numEpochs","cursor_increment" to "cursorIncrement","min_distance" to "minDataDistance","max_distance" to "maxDataDistance","initial_adjust_rate" to "initialAdjustRate","adjust_rate_decay" to "adjustRateDecay","randomize" to "randomizeFrameOrder","seed" to "randSeed","scale_each_step" to "scaleEachStep","filter_outliers" to "useFrameFiltering","slide_factor" to "slideErrorFactor","offset_slide_factor" to "offsetSlideErrorFactor","foot_height_factor" to "footHeightOffsetErrorFactor","proportion_factor" to "bodyProportionErrorFactor","height_factor" to "heightErrorFactor","position_factor" to "positionErrorFactor","position_offset_factor" to "positionOffsetErrorFactor","max_final_error" to "maxFinalError","calc_initial_error" to "calcInitError","use_skeleton_height" to "useSkeletonHeight")
fun main() {
    for(line in generateSequence(::readLine)) {
        val n=mapper.readTree(line);val config=AutoBoneConfig()
        for((key,name)in fields) {
            val field=AutoBoneConfig::class.java.getDeclaredField(name);field.isAccessible=true
            val node=n["config"][key]
            val value:Any=when(field.type){java.lang.Integer.TYPE->node.intValue();java.lang.Long.TYPE->node.longValue();java.lang.Float.TYPE->node.floatValue();else->node.booleanValue()}
            field.set(config,value)
        }
        val bodies=n["frames"].flatMap{it["rotations"].properties().map{it.key}}.toSet()+"head"
        val frames=PoseFrames(bodies.map{name ->
            TrackerFrames(n["frames"].map {frame ->
                val head=name=="head"
                val rotation=if(head)frame["head"]["rotation"] else frame["rotations"][name]
                val position=if(head)frame["head"]["position"]else frame["positions"][name]
                TrackerFrame(TrackerPosition.valueOf(name.uppercase()),rotation?.takeUnless{it.isNull}?.let{quaternion(it)},position?.takeUnless{it.isNull}?.let{vector(it)})
            }.toMutableList())
        })
        val manager=ConfigManager(OracleVRConfig(n["initial"] as ObjectNode,config))
        manager.vrConfig.skeleton.userHeight=if(n["target_height"].isNull)0f else n["target_height"].floatValue()
        val skeleton=HumanPoseManager(emptyList());skeleton.loadFromConfig(manager)
        val bone=AutoBone(OracleServer(manager,skeleton))
        val target=if(manager.vrConfig.skeleton.userHeight > .4f)manager.vrConfig.skeleton.userHeight else bone.calcTargetHmdHeight(frames,config)
        val epochs=mutableListOf<Any>();var accepted=true
        try {bone.processFrames(frames,epochCallback={epoch ->
            val count=StatsCalculator::class.java.getDeclaredField("count").apply{isAccessible=true}.getInt(epoch.epochError)
            epochs+=mapOf("epoch" to epoch.epoch,"mean_error" to epoch.epochError.mean,"standard_deviation" to epoch.epochError.standardDeviation,"steps" to count,"offsets" to epoch.configValues.entries.associate{OFFSET_FIELDS.getValue(it.key) to it.value})
        })} catch(e:AutoBoneException) {accepted=false}
        println(mapper.writeValueAsString(mapOf("estimated_height" to bone.estimatedHeight,"target_height" to target,"accepted" to accepted,"offsets" to bone.offsets.entries.associate{OFFSET_FIELDS.getValue(it.key) to it.value},"epochs" to epochs,"frames_used" to frames.maxFrameCount)))
    }
}
