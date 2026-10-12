#include <gtk/gtk.h>
#include <webkit2/webkit2.h>
#include <stdio.h>
#include <string.h>

static const char page[] =
    "<!doctype html><html><head><meta charset='utf-8'><style>"
    "*{box-sizing:border-box}body{margin:0;color:#eee;font:16px sans-serif;background:transparent}"
    ".scene{position:fixed;inset:0;background:radial-gradient(at 70% 20%,#744793,transparent 60%),"
    "linear-gradient(145deg,rgba(20,25,35,.8),rgba(8,12,20,.2))}"
    "aside{position:fixed;inset:12px auto 12px 12px;width:190px;padding:24px;border-radius:14px;"
    "background:rgba(28,30,40,.15);backdrop-filter:blur(6px)}"
    "main{position:relative;margin:12px 12px 12px 216px}"
    ".hero{height:300px;overflow:hidden;border-radius:14px;background:linear-gradient(30deg,#163e58,#933866);"
    "position:relative;isolation:isolate}.art{position:absolute;inset:0;transform:translateZ(0);"
    "background:repeating-linear-gradient(55deg,transparent 0 25px,#ffffff15 26px 27px)}"
    ".glass{background:rgba(40,40,50,.15);backdrop-filter:blur(8px);border-radius:14px;padding:20px}"
    ".hero .glass{position:absolute;bottom:16px;left:16px;right:16px}"
    ".cards{display:grid;grid-template-columns:repeat(3,1fr);gap:12px;margin-top:12px}"
    ".card{height:150px;transition:transform .2s,background .2s}.card:hover{transform:scale(1.02);background:#9993}"
    "button{padding:10px;border:1px solid #aaa5;border-radius:8px;background:#18182088;color:white;cursor:pointer}"
    "dialog{color:#eee;background:#151521aa;backdrop-filter:blur(12px);border:1px solid #eee5;border-radius:14px}"
    "dialog::backdrop{background:#0007}dialog .hero{width:75vw;height:65vh}"
    "@keyframes fade{from{opacity:.2}to{opacity:1}}.fade{animation:fade .35s}"
    "</style></head><body><div class='scene'></div><aside><b>Legio rendering probe</b>"
    "<p>Sidebar blur: 6 px</p><p>Switch focus/workspace, minimize, restore and resize.</p>"
    "<button onclick=\"document.querySelector('main').classList.remove('fade');"
    "requestAnimationFrame(()=>document.querySelector('main').classList.add('fade'))\">Fade content</button></aside>"
    "<main><section class='hero'><div class='art'></div><div class='glass'><h1>Game detail</h1>"
    "<button onclick=\"document.querySelector('dialog').showModal()\">Expand artwork</button></div></section>"
    "<div class='cards'><div class='glass card'>Overview</div><div class='glass card'>Game information</div>"
    "<div class='glass card'>Actions</div><div class='glass card'>Screenshots</div>"
    "<div class='glass card'>Metadata</div><div class='glass card'>Library</div></div></main>"
    "<dialog><div class='hero'><div class='art'></div></div>"
    "<button onclick=\"this.closest('dialog').close()\">Close artwork</button></dialog></body></html>";

static gboolean set_feature(WebKitSettings *settings, WebKitFeatureList *features,
                            const char *identifier, gboolean enabled) {
    WebKitFeature *feature = webkit_feature_list_find(features, identifier);
    if (!feature) {
        fprintf(stderr, "Unavailable feature: %s\n", identifier);
        return FALSE;
    }
    webkit_settings_set_feature_enabled(settings, feature, enabled);
    if (webkit_settings_get_feature_enabled(settings, feature) != enabled) {
        fprintf(stderr, "Preference not applied: %s\n", identifier);
        return FALSE;
    }
    printf("%s=%s\n", identifier, enabled ? "true" : "false");
    return TRUE;
}

static void process_terminated(WebKitWebView *view, WebKitWebProcessTerminationReason reason,
                               gpointer data) {
    (void)view;
    (void)data;
    fprintf(stderr, "WebProcess terminated: %d\n", reason);
}

int main(int argc, char **argv) {
    gboolean damage = TRUE, skia = TRUE, opaque = FALSE, check = FALSE;
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--full-composition")) damage = FALSE;
        else if (!strcmp(argv[i], "--texture-mapper")) skia = FALSE;
        else if (!strcmp(argv[i], "--opaque")) opaque = TRUE;
        else if (!strcmp(argv[i], "--check")) check = TRUE;
        else if (!strcmp(argv[i], "--help")) {
            puts("probe [--full-composition] [--texture-mapper] [--opaque] [--check]");
            return 0;
        } else {
            fprintf(stderr, "Unknown option: %s\n", argv[i]);
            return 2;
        }
    }
    const char *override = g_getenv("WEBKIT_USE_SKIA_FOR_COMPOSITION");
    if (override && strcmp(override, skia ? "1" : "0")) {
        fputs("Conflicting WEBKIT_USE_SKIA_FOR_COMPOSITION; unset it before using this probe\n", stderr);
        return 2;
    }
    printf("WebKitGTK=%u.%u.%u\n", webkit_get_major_version(), webkit_get_minor_version(),
           webkit_get_micro_version());
    if (webkit_get_major_version() < 2 ||
        (webkit_get_major_version() == 2 && webkit_get_minor_version() < 54)) {
        fputs("This probe requires WebKitGTK 2.54 or newer\n", stderr);
        return 3;
    }
    if (!gtk_init_check(NULL, NULL)) {
        fputs("GTK display unavailable\n", stderr);
        return 4;
    }
    printf("backend=%s; native_alpha=%s\n", G_OBJECT_TYPE_NAME(gdk_display_get_default()),
           opaque ? "false" : "true");
    g_autoptr(WebKitSettings) settings = webkit_settings_new();
    g_autoptr(WebKitFeatureList) features = webkit_settings_get_all_features();
    if (!features || !set_feature(settings, features, "UseSkiaForComposition", skia) ||
        !set_feature(settings, features, "UseDamagingInformationForCompositing", damage)) return 5;
    /* Construct with initial preferences and an ephemeral context before WebPage creation. */
    GtkWidget *view = g_object_new(WEBKIT_TYPE_WEB_VIEW, "settings", settings,
                                   "is-ephemeral", TRUE, NULL);
    if (check) {
        g_object_ref_sink(view);
        g_object_unref(view);
        return 0;
    }
    GtkWidget *window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_window_set_default_size(GTK_WINDOW(window), 1000, 700);
    gtk_window_set_title(GTK_WINDOW(window), "Legio native rendering probe");
    if (!opaque) {
        GdkVisual *visual = gdk_screen_get_rgba_visual(gtk_widget_get_screen(window));
        if (!visual) {
            fputs("RGBA visual unavailable; cannot test native alpha\n", stderr);
            gtk_widget_destroy(window);
            g_object_ref_sink(view);
            g_object_unref(view);
            return 6;
        }
        gtk_widget_set_visual(window, visual);
        gtk_widget_set_app_paintable(window, TRUE);
        GdkRGBA clear = {0, 0, 0, 0};
        webkit_web_view_set_background_color(WEBKIT_WEB_VIEW(view), &clear);
    }
    gtk_container_add(GTK_CONTAINER(window), view);
    g_signal_connect(window, "destroy", G_CALLBACK(gtk_main_quit), NULL);
    g_signal_connect(view, "web-process-terminated", G_CALLBACK(process_terminated), NULL);
    webkit_web_view_load_html(WEBKIT_WEB_VIEW(view), page, "about:blank");
    gtk_widget_show_all(window);
    gtk_main();
    return 0;
}
