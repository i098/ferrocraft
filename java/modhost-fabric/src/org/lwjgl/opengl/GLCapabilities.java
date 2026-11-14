package org.lwjgl.opengl;

/**
 * What Argentum and OneConfig read of LWJGL 3's {@code GLCapabilities} (the fields their classes name). The host has no GL
 * context (Rust draws), so every capability reads as present and the mods go on to start; nothing of theirs reaches a GPU.
 */
public final class GLCapabilities {
    public final boolean GL_AMD_debug_output = true;
    public final boolean GL_APPLE_vertex_array_object = true;
    public final boolean GL_ARB_base_instance = true;
    public final boolean GL_ARB_buffer_storage = true;
    public final boolean GL_ARB_compatibility = true;
    public final boolean GL_ARB_copy_buffer = true;
    public final boolean GL_ARB_debug_output = true;
    public final boolean GL_ARB_direct_state_access = true;
    public final boolean GL_ARB_draw_elements_base_vertex = true;
    public final boolean GL_ARB_draw_instanced = true;
    public final boolean GL_ARB_framebuffer_object = true;
    public final boolean GL_ARB_instanced_arrays = true;
    public final boolean GL_ARB_map_buffer_range = true;
    public final boolean GL_ARB_multi_draw_indirect = true;
    public final boolean GL_ARB_shader_storage_buffer_object = true;
    public final boolean GL_ARB_sync = true;
    public final boolean GL_ARB_texture_filter_anisotropic = true;
    public final boolean GL_ARB_texture_float = true;
    public final boolean GL_ARB_texture_storage = true;
    public final boolean GL_ARB_timer_query = true;
    public final boolean GL_ARB_uniform_buffer_object = true;
    public final boolean GL_ARB_vertex_array_object = true;
    public final boolean GL_EXT_framebuffer_object = true;
    public final boolean GL_EXT_gpu_shader4 = true;
    public final boolean GL_EXT_texture_array = true;
    public final boolean GL_EXT_texture_filter_anisotropic = true;
    public final boolean GL_EXT_timer_query = true;
    public final boolean GL_KHR_debug = true;
    public final boolean OpenGL11 = true;
    public final boolean OpenGL12 = true;
    public final boolean OpenGL13 = true;
    public final boolean OpenGL14 = true;
    public final boolean OpenGL15 = true;
    public final boolean OpenGL20 = true;
    public final boolean OpenGL21 = true;
    public final boolean OpenGL30 = true;
    public final boolean OpenGL31 = true;
    public final boolean OpenGL32 = true;
    public final boolean OpenGL33 = true;
    public final boolean OpenGL40 = true;
    public final boolean OpenGL41 = true;
    public final boolean OpenGL42 = true;
    public final boolean OpenGL43 = true;
    public final boolean OpenGL44 = true;
    public final boolean OpenGL45 = true;
    public final boolean OpenGL46 = true;
}
