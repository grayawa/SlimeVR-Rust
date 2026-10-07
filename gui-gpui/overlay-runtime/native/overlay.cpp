// Narrow C ABI around the pinned OpenVR SDK. No linked OpenVR library, STL,
// exceptions or global SDK context. Each session owns its DLL and handles.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <openvr.h>
#include <cstdio>
#include <cstdlib>

using Init = uint32_t (*)(vr::EVRInitError *, vr::EVRApplicationType, const char *);
using Shutdown = void (*)();
using Interface = void *(*)(const char *, vr::EVRInitError *);
struct Session {
    HMODULE library;
    Shutdown shutdown;
    vr::IVRSystem *system;
    vr::IVROverlay *overlay;
    vr::VROverlayHandle_t main_handle, thumbnail;
    bool started;
};
struct Event { uint32_t kind, button; float x, y, dx, dy; };
static_assert(sizeof(Event) == 24);
static void fail(char *buffer, size_t size, const char *operation, int code) {
    std::snprintf(buffer, size, "%s failed (%d)", operation, code);
}
extern "C" void svro_close(Session *s) {
    if (!s) return;
    if (s->overlay) {
        if (s->main_handle) { s->overlay->ClearOverlayTexture(s->main_handle); s->overlay->DestroyOverlay(s->main_handle); }
        if (s->thumbnail) s->overlay->DestroyOverlay(s->thumbnail);
    }
    if (s->started && s->shutdown) s->shutdown();
    if (s->library) FreeLibrary(s->library);
    std::free(s);
}
extern "C" Session *svro_open(const wchar_t *dll, const char *key, const char *title, float width, char *error, size_t capacity) {
    auto *s = static_cast<Session *>(std::calloc(1, sizeof(Session)));
    if (!s) { fail(error, capacity, "allocate session", 0); return nullptr; }
    s->library = LoadLibraryExW(dll, nullptr, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (!s->library) { fail(error, capacity, "load openvr_api.dll", int(GetLastError())); svro_close(s); return nullptr; }
    auto init = reinterpret_cast<Init>(GetProcAddress(s->library, "VR_InitInternal2"));
    auto get = reinterpret_cast<Interface>(GetProcAddress(s->library, "VR_GetGenericInterface"));
    s->shutdown = reinterpret_cast<Shutdown>(GetProcAddress(s->library, "VR_ShutdownInternal"));
    if (!init || !get || !s->shutdown) { fail(error, capacity, "OpenVR exports", 0); svro_close(s); return nullptr; }
    vr::EVRInitError status = vr::VRInitError_None;
    init(&status, vr::VRApplication_Overlay, nullptr);
    if (status != vr::VRInitError_None) { fail(error, capacity, "initialize SteamVR overlay", int(status)); svro_close(s); return nullptr; }
    s->started = true;
    s->system = static_cast<vr::IVRSystem *>(get(vr::IVRSystem_Version, &status));
    if (!s->system || status != vr::VRInitError_None) { fail(error, capacity, "IVRSystem", int(status)); svro_close(s); return nullptr; }
    s->overlay = static_cast<vr::IVROverlay *>(get(vr::IVROverlay_Version, &status));
    if (!s->overlay || status != vr::VRInitError_None) { fail(error, capacity, "IVROverlay", int(status)); svro_close(s); return nullptr; }
    vr::VROverlayHandle_t main_handle = 0, thumbnail = 0;
    auto result = s->overlay->CreateDashboardOverlay(key, title, &main_handle, &thumbnail);
    if (result != vr::VROverlayError_None) { fail(error, capacity, "create dashboard (another instance may be running)", int(result)); svro_close(s); return nullptr; }
    s->main_handle = main_handle;
    s->thumbnail = thumbnail;
    result = s->overlay->SetOverlayWidthInMeters(s->main_handle, width);
    if (result == vr::VROverlayError_None) result = s->overlay->SetOverlayInputMethod(s->main_handle, vr::VROverlayInputMethod_Mouse);
    if (result == vr::VROverlayError_None) result = s->overlay->SetOverlayFlag(s->main_handle, vr::VROverlayFlags_SendVRDiscreteScrollEvents, true);
    if (result != vr::VROverlayError_None) { fail(error, capacity, "configure dashboard", int(result)); svro_close(s); return nullptr; }
    return s;
}
extern "C" int svro_visible(Session *s) { return s->overlay->IsOverlayVisible(s->main_handle) ? 1 : 0; }
extern "C" int svro_adapter(Session *s) {
    int index = -1;
    s->system->GetDXGIOutputInfo(&index);
    return index;
}
extern "C" int svro_submit(Session *s, void *texture, uint32_t width, uint32_t height) {
    vr::HmdVector2_t scale{{float(width), float(height)}};
    auto result = s->overlay->SetOverlayMouseScale(s->main_handle, &scale);
    if (result != vr::VROverlayError_None) return int(result);
    vr::Texture_t value{texture, vr::TextureType_DirectX, vr::ColorSpace_Auto};
    return int(s->overlay->SetOverlayTexture(s->main_handle, &value));
}
extern "C" int svro_thumbnail(Session *s, void *rgba, uint32_t width, uint32_t height) {
    return int(s->overlay->SetOverlayRaw(s->thumbnail, rgba, width, height, 4));
}
extern "C" int svro_poll(Session *s, Event *output) {
    vr::VREvent_t event{};
    // Each call consumes one event, including ignored events, so Rust can cap
    // work per turn even if the runtime queue is flooded.
    bool from_main = s->overlay->PollNextOverlayEvent(s->main_handle, &event, sizeof(event));
    if (!from_main) {
        if (!s->overlay->PollNextOverlayEvent(s->thumbnail, &event, sizeof(event))) {
            if (!s->system->PollNextEvent(&event, sizeof(event))) return 0;
        }
    }
    *output = {};
    if (!from_main && event.eventType != vr::VREvent_Quit) return 1;
    switch (event.eventType) {
    case vr::VREvent_MouseMove: output->kind = 1; break;
    case vr::VREvent_MouseButtonDown: output->kind = 2; break;
    case vr::VREvent_MouseButtonUp: output->kind = 3; break;
    case vr::VREvent_ScrollDiscrete: case vr::VREvent_ScrollSmooth:
        output->kind = 4; output->dx = event.data.scroll.xdelta; output->dy = event.data.scroll.ydelta; break;
    case vr::VREvent_OverlayShown: output->kind = 5; break;
    case vr::VREvent_OverlayHidden: output->kind = 6; break;
    case vr::VREvent_Quit: s->system->AcknowledgeQuit_Exiting(); output->kind = 7; break;
    case vr::VREvent_FocusLeave: output->kind = 8; break;
    default: break;
    }
    if (output->kind >= 1 && output->kind <= 3) {
        output->x = event.data.mouse.x; output->y = event.data.mouse.y; output->button = event.data.mouse.button;
    }
    return 1;
}
