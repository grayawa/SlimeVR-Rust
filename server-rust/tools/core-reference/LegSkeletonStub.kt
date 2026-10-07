package dev.slimevr.tracking.processor.skeleton
import dev.slimevr.tracking.processor.*
import dev.slimevr.tracking.processor.config.SkeletonConfigToggles
import dev.slimevr.tracking.trackers.*
class HumanSkeleton {
    var bones:Map<BoneType,Bone> = emptyMap()
    var trackers:Map<TrackerPosition,Tracker> = emptyMap()
    var computed:Map<String,Tracker> = emptyMap()
    val humanPoseManager=LegPoseManager()
    // LEG_BONES
    // LEG_TRACKERS
    // COMPUTED_TRACKERS
    val hasKneeTrackers get()=leftUpperLegTracker!=null&&rightUpperLegTracker!=null
    val hasLeftArmTracker get()=leftUpperArmTracker!=null||leftLowerArmTracker!=null
    val hasRightArmTracker get()=rightUpperArmTracker!=null||rightLowerArmTracker!=null
    lateinit var legTweaks:LegTweaks
}
class LegPoseManager {
    var toggles=mapOf<SkeletonConfigToggles,Boolean>()
    fun getToggle(t:SkeletonConfigToggles)=toggles[t] ?: false
}
