package dev.slimevr.config
enum class ArmsResetModes {BACK,FORWARD,TPOSE_UP,TPOSE_DOWN}
class DriftCompensationConfig {var prediction=false;var amount=0.8f;var maxResets=6}
class ResetsConfig {var mode=ArmsResetModes.BACK;var yawResetSmoothTime=0f;var saveMountingReset=false;var resetHmdPitch=false}
