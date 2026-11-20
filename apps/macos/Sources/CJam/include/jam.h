#ifndef JAM_H
#define JAM_H
#include <stdint.h>
/* Pinned libghostty-vt modifier ABI, shared with Request::Key/Mouse. */
#define JAM_MOD_SHIFT 1
#define JAM_MOD_CONTROL 2
#define JAM_MOD_ALT 4
#define JAM_MOD_SUPER 8
/* UTF-8 input strings are borrowed for the call. Returned strings are owned by
 * the caller and must be released exactly once with jam_free. RPC/start may
 * block and belong on a worker queue. View calls belong on AppKit's main thread.
 * A view handle borrows its NSView until jam_view_drop; drop it exactly once. */
void jam_free(char *p);
char *jam_config(const char *request);
char *jam_start(const char *path);
char *jam_request(const char *request);
char *jam_request_endpoint(const char *endpoint, const char *request);
char *jam_remote(const char *request);
char *jam_agent_integrations(const char *request);
char *jam_server(const char *request);
/* Worker-queue only. launch requires confirmed=true after interruption consent. */
char *jam_upgrade(const char *request);
void *jam_view_new(void *view, const char *pane, uint32_t width, uint32_t height);
void jam_view_invalidate(void *view);
char *jam_view_draw(void *view, uint32_t width, uint32_t height, float font_scale);
void jam_view_set_colors(void *view, uint32_t foreground, uint32_t background);
void jam_view_drop(void *view);
char *jam_view_action(void *view, const char *action, int32_t x, int32_t y);
char *jam_view_find(void *view, const char *request);
#endif
