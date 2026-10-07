package dev.slimevr.tracking.processor
import com.fasterxml.jackson.databind.ObjectMapper
import com.fasterxml.jackson.databind.node.ObjectNode
import dev.slimevr.config.ConfigManager
import dev.slimevr.tracking.processor.config.*
import dev.slimevr.tracking.processor.skeleton.HumanSkeleton
import dev.slimevr.tracking.trackers.*
import SkeletonReference
import q
import v
class HumanPoseManager(val trackers:List<Tracker>) {
    val skeleton=HumanSkeleton()
    val reference=SkeletonReference()
    val offsets=SkeletonConfigManager(false)
    lateinit var pose:ObjectNode
    fun loadFromConfig(config:ConfigManager) {
        pose=config.vrConfig.pose.deepCopy()
        for(offset in SkeletonConfigOffsets.values) offsets.values[offset]=pose[OFFSET_FIELDS.getValue(offset)].floatValue()
    }
    fun getOffset(offset:SkeletonConfigOffsets)=offsets.getOffset(offset)
    fun setOffset(offset:SkeletonConfigOffsets,value:Float) { offsets.values[offset]=value }
    val userHeightFromConfig get()=SkeletonConfigManager.HEIGHT_OFFSETS.fold(0f){sum,offset->sum+getOffset(offset)}
    fun getBone(type:BoneType)=skeleton.getBone(type)
    fun getComputedTracker(role:TrackerRole)=skeleton.getComputedTracker(role)
    fun setLegTweaksEnabled(enabled:Boolean) {}
    fun update() {
        val mapper=ObjectMapper();val node=mapper.createObjectNode()
        for((offset,value) in offsets.values)pose.put(OFFSET_FIELDS.getValue(offset),value)
        node.set<ObjectNode>("config",pose)
        val inputs=node.putObject("inputs");val positions=node.putObject("positions")
        var head:ObjectNode?=null
        for(tracker in trackers) {
            val name=tracker.trackerPosition!!.name.lowercase()
            if(tracker.trackerPosition==TrackerPosition.HEAD) {
                head=mapper.createObjectNode();head.set<ObjectNode>("rotation",mapper.valueToTree(q(tracker.getRotation())))
                if(tracker.hasPosition)head.set<ObjectNode>("position",mapper.valueToTree(v(tracker.position)))else head.putNull("position")
            } else {
                inputs.set<ObjectNode>(name,mapper.valueToTree(q(tracker.getRotation())))
                if(tracker.hasPosition)positions.set<ObjectNode>(name,mapper.valueToTree(v(tracker.position)))
            }
        }
        if(head==null)node.putNull("head")else node.set<ObjectNode>("head",head)
        val nodes=node.putArray("offsets")
        for(type in BoneType.values)offsets.computeNodeOffset(type)
        for((type,offset)in offsets.nodes) nodes.addObject().put("name",type.name.lowercase()).set<ObjectNode>("offset",mapper.valueToTree(v(offset)))
        reference.run(node)
        skeleton.bones=reference.referenceBones()
        for(role in TrackerRole.entries) {
            val bone=ROLE_BONES[role] ?: continue
            val source=skeleton.getBone(bone)
            skeleton.computed[role]=Tracker().apply{position=source.getTailPosition();hasPosition=true}
        }
    }
}
