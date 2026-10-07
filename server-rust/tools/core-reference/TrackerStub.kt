package dev.slimevr.tracking.trackers
import dev.slimevr.filtering.QuaternionMovingAverage
import dev.slimevr.filtering.TrackerFilters
import dev.slimevr.tracking.trackers.udp.TrackerDataType
import dev.slimevr.util.InterpolationHandler
import io.github.axisangles.ktmath.Quaternion
import io.github.axisangles.ktmath.Vector3

// Ordinary rotational trackers only; disabled external callbacks are explicit no-ops.
class Tracker(var trackerPosition:TrackerPosition?=null) {
    var trackerDataType=TrackerDataType.ROTATION
    var isHmd=false;var isComputed=false;var isInternal=false
    var allowMounting=true;var needReset=true;var hasPosition=false;var hasRotation=true
    var position=Vector3.NULL
    var referenceAcceleration=Vector3.NULL
    fun dataTick() {}
    fun getAcceleration()=referenceAcceleration
    fun setAcceleration(a:Vector3){referenceAcceleration=a}
    fun setRotation(q:Quaternion){directRotation=q}
    var raw=Quaternion.IDENTITY
    val resetsHandler=TrackerResetsHandler(this)
    val yawResetSmoothing=InterpolationHandler()
    val stayAligned=dev.slimevr.tracking.processor.stayaligned.trackers.StayAlignedTrackerState(this)
    val trackerFlexHandler=DisabledFlex()
    val filteringHandler=ReferenceFiltering()
    var imuType=dev.slimevr.tracking.trackers.udp.IMUType.UNKNOWN
    var magStatus=dev.slimevr.tracking.trackers.udp.MagnetometerStatus.NOT_SUPPORTED
    fun isImu()=!isHmd&&!isComputed
    fun getAdjustedRotationForceStayAligned()=Quaternion.rotationAroundYAxis(stayAligned.yawCorrection.toRad())*(directRotation ?: resetsHandler.getReferenceAdjustedDriftRotationFrom(raw))
    var directRotation:Quaternion?=null
    fun getRawRotation()=raw
    fun getRotation()=directRotation ?: (yawResetSmoothing.curRotation*filteringHandler.average.filteredQuaternion)
    fun resetFilteringQuats(reference:Quaternion) {filteringHandler.average.resetQuats(resetsHandler.getReferenceAdjustedDriftRotationFrom(raw),reference)}
    fun saveMountingResetOrientation(q:Quaternion) {}
}
class ReferenceFiltering {
    var average=QuaternionMovingAverage(TrackerFilters.NONE)
    fun getFilteringImpact()=average.filteringImpact
}
class DisabledStayAligned {fun reset() {}}
class DisabledFlex {fun resetMin() {};fun resetMax() {}}
object TrackerUtils {
    fun getNonInternalNonImuTrackerForBodyPosition(trackers:List<Tracker>,position:TrackerPosition):Tracker?=null
}
