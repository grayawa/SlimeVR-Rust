import com.fasterxml.jackson.databind.JsonNode
import dev.slimevr.tracking.processor.*
import dev.slimevr.tracking.processor.Constraint.Companion.ConstraintType
import dev.slimevr.tracking.trackers.*
import dev.slimevr.tracking.trackers.udp.TrackerDataType
import io.github.axisangles.ktmath.*
import io.github.axisangles.ktmath.Quaternion.Companion.IDENTITY
import io.github.axisangles.ktmath.Quaternion.Companion.I
import io.github.axisangles.ktmath.Quaternion.Companion.fromTo
import io.github.axisangles.ktmath.Vector3.Companion.NULL
import io.github.axisangles.ktmath.Vector3.Companion.POS_Y
import io.github.axisangles.ktmath.Vector3.Companion.NEG_Y

// Methods marked EXTRACTED below are copied verbatim by the generator, never reimplemented.
// Constraints, arms, LegTweaks and Localizer are disabled for this subset reference.
class SkeletonReference {
    private val bones=mapOf(
// BONE_MAP
)
    // BONE_FIELDS
    // TRACKER_FIELDS
    private val localizer=DisabledLocalizer()
    private val isTrackingLeftArmFromController get()=leftHandTracker?.hasPosition==true&&!forceArmsFromHMD
    private val isTrackingRightArmFromController get()=rightHandTracker?.hasPosition==true&&!forceArmsFromHMD
    private var hasSpineTracker=false
    private var hasKneeTrackers=false
    private var extendedSpineModel=true
    private var extendedPelvisModel=true
    private var extendedKneeModel=true
    private var waistFromChestHipAveraging=0.30f
    private var waistFromChestLegsAveraging=0.30f
    private var hipFromChestLegsAveraging=0.50f
    private var hipFromWaistLegsAveraging=0.40f
    private var hipLegsAveraging=0.25f
    private var kneeTrackerAnkleAveraging=0.85f
    private var kneeAnkleAveraging=0.0f
    private fun getBone(type:BoneType)=bones.getValue(type)
    private fun getFirstAvailableTracker(vararg trackers:Tracker?)=trackers.firstOrNull {it!=null}
    private var forceArmsFromHMD=true
    private val allArmBones get()=arrayOf(// ALL_ARM_BONES
)
    // EXTRACTED_METHODS

    fun enableLocalizer(){localizer.active=true}
    fun referenceBones()=bones
    fun run(n:JsonNode):Any {
        forceArmsFromHMD=n["config"]["force_arms_from_hmd"].booleanValue()
        // SET_TRACKERS
        for ((name,entry) in (n["positions"]?.fields()?.asSequence() ?: emptySequence())) { val tracker=when(name){"left_hand"->leftHandTracker;"right_hand"->rightHandTracker;else->null};tracker?.hasPosition=true;tracker?.position=vector(entry)}
        hasSpineTracker=listOf(upperChestTracker,chestTracker,waistTracker,hipTracker).any {it!=null}
        hasKneeTrackers=leftUpperLegTracker!=null&&rightUpperLegTracker!=null
        extendedSpineModel=n["config"]["extended_spine"].booleanValue()
        extendedPelvisModel=n["config"]["extended_pelvis"].booleanValue()
        extendedKneeModel=n["config"]["extended_knee"].booleanValue()
        // SET_RATIOS
        val head=n["head"]
        if(!head.isNull) {
            headTracker=Tracker(TrackerPosition.HEAD)
            headTracker!!.directRotation=quaternion(head["rotation"])
            headTracker!!.hasPosition=!head["position"].isNull
            if(headTracker!!.hasPosition) headTracker!!.position=vector(head["position"])
        }
        for(entry in n["offsets"]) updateNodeOffset(BoneType.valueOf(entry["name"].asText().uppercase()),vector(entry["offset"]))
        if(neckBone.parent==null) assembleSkeleton() else assembleSkeletonArms(true)
        updateHeadTransforms();updateSpineTransforms()
        updateLegTransforms(leftUpperLegBone,leftKneeTrackerBone,leftLowerLegBone,leftFootBone,leftFootTrackerBone,leftUpperLegTracker,leftLowerLegTracker,leftFootTracker)
        updateLegTransforms(rightUpperLegBone,rightKneeTrackerBone,rightLowerLegBone,rightFootBone,rightFootTrackerBone,rightUpperLegTracker,rightLowerLegTracker,rightFootTracker)
        // RUN_ARMS
        // RUN_FINGERS
        headBone.update()
        if(isTrackingLeftArmFromController) leftHandTrackerBone.update()
        if(isTrackingRightArmFromController) rightHandTrackerBone.update()
        if(n["config"]["enforce_constraints"].booleanValue()) headBone.updateWithConstraints(false)
        return n["offsets"].associate {entry ->
            val name=entry["name"].asText();val bone=getBone(BoneType.valueOf(name.uppercase()))
            name to mapOf("head" to v(bone.getPosition()),"tail" to v(bone.getTailPosition()),"rotation" to q(bone.getGlobalRotation()),"rotation_offset" to q(bone.rotationOffset),"length" to bone.length)
        }
    }
}
class DisabledLocalizer {var active=false;fun getEnabled()=active}
