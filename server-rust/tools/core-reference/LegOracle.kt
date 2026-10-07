import com.fasterxml.jackson.databind.JsonNode
import dev.slimevr.config.LegTweaksConfig
import dev.slimevr.tracking.processor.skeleton.*
import dev.slimevr.tracking.processor.config.SkeletonConfigToggles
import dev.slimevr.tracking.trackers.Tracker
import dev.slimevr.tracking.trackers.TrackerPosition
object OracleClock {var nanos=0L}
fun legReference(n:JsonNode):Any {
    OracleClock.nanos=0
    val skeleton=HumanSkeleton()
    skeleton.humanPoseManager.toggles=mapOf(
        SkeletonConfigToggles.FLOOR_CLIP to n["legs"]["floor_clip"].booleanValue(),
        SkeletonConfigToggles.SKATING_CORRECTION to n["legs"]["skating"].booleanValue(),
        SkeletonConfigToggles.TOE_SNAP to n["legs"]["toe_snap"].booleanValue(),
        SkeletonConfigToggles.FOOT_PLANT to n["legs"]["foot_plant"].booleanValue()
    )
    val cfg=LegTweaksConfig();cfg.correctionStrength=n["legs"]["correction_strength"].floatValue()
    val legs=LegTweaks(skeleton,cfg);skeleton.legTweaks=legs
    if(n["reset_floor"].booleanValue()) legs.resetFloorLevel()
    val reference=SkeletonReference()
    skeleton.bones=reference.referenceBones()
    val localizer=Localizer(skeleton)
    if(n.path("localizer").booleanValue()){reference.enableLocalizer();localizer.setEnabled(true);localizer.reset()}
    return n["frames"].map {frame ->
        OracleClock.nanos=frame["at_ms"].longValue()*1000000L
        reference.run(frame)
        skeleton.bones=reference.referenceBones()
        skeleton.trackers=frame["inputs"].properties().associate {entry ->TrackerPosition.valueOf(entry.key.uppercase()) to Tracker(TrackerPosition.valueOf(entry.key.uppercase())).apply {directRotation=quaternion(entry.value);referenceAcceleration=frame["accelerations"][entry.key]?.let(::vector) ?: io.github.axisangles.ktmath.Vector3.NULL}}
        skeleton.computed=reference.referenceBones().filter {it.key.name.endsWith("_TRACKER")}.mapKeys {it.key.name.lowercase().removeSuffix("_tracker")}.mapValues {(_,bone)->Tracker().apply{position=bone.getTailPosition();directRotation=bone.getGlobalRotation()*bone.rotationOffset.inv()}}
        legs.tweakLegs()
        localizer.update()
        val b=legs.bufferHead
        mapOf("root" to v(skeleton.headBone.getPosition()),"floor" to legs.floorLevel,"feet" to listOf(v(b.leftFootPosition),v(b.rightFootPosition)),"knees" to listOf(v(b.leftKneePosition),v(b.rightKneePosition)),"hip" to v(b.hipPosition),"corrected_feet" to listOf(v(b.leftFootPositionCorrected),v(b.rightFootPositionCorrected)),"corrected_knees" to listOf(v(b.leftKneePositionCorrected),v(b.rightKneePositionCorrected)),"corrected_hip" to v(b.hipPositionCorrected),"corrected_rotations" to listOf(q(skeleton.computed.getValue("left_foot").getRotation()),q(skeleton.computed.getValue("right_foot").getRotation())),"state" to listOf(b.leftLegState,b.rightLegState),"numerical" to listOf(b.leftLegNumericalState,b.rightLegNumericalState),"com" to v(b.centerOfMass),"standing" to b.isStanding)
    }
}
