package dev.slimevr.tracking.processor.skeleton
import dev.slimevr.tracking.processor.*
import dev.slimevr.tracking.trackers.*
class HumanSkeleton {
    var bones:Map<BoneType,Bone> = emptyMap()
    val computed=mutableMapOf<TrackerRole,Tracker>()
    fun getBone(type:BoneType)=bones.getValue(type)
    fun getComputedTracker(role:TrackerRole)=computed.getValue(role)
}
