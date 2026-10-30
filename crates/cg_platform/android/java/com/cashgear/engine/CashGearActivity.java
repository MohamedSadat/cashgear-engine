package com.cashgear.engine;

import android.app.NativeActivity;
import android.os.Build;
import android.os.Bundle;
import android.os.Process;
import android.view.DisplayCutout;
import android.view.View;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.window.OnBackInvokedCallback;
import android.window.OnBackInvokedDispatcher;

/**
 * The activity of a CashGear Engine game on Android.
 *
 * <p>A {@link NativeActivity}: the game runs in the native library named by the manifest's
 * {@code android.app.lib_name}, whose {@code android_main} comes from
 * {@code cashgear_engine::android_main!}. Around it, this class does what Android games
 * need: it hides the system bars (immersive mode) and keeps the screen on, lays the window
 * out under display cutouts and records their safe insets, counts presses of the back
 * button or gesture instead of closing the app, and keeps the launching intent's
 * {@code args} extra as launch flags. The engine reads those fields through JNI
 * ({@code cg_platform::android}).
 *
 * <p>A game uses it as is: its manifest names {@code com.cashgear.engine.CashGearActivity}
 * and its Gradle build adds this folder to the Java sources.
 */
public class CashGearActivity extends NativeActivity {
    /** Presses of the back button or gesture since the activity started. */
    volatile int backPresses;

    /**
     * The display cutout's safe insets in pixels of the window, 16 bits each from the
     * lowest: left, top, right, bottom.
     */
    volatile long safeInsets;

    /** The launching intent's {@code args} extra, such as {@code "--demo"}; empty without. */
    volatile String launchArgs = "";

    /** The Android 13+ back callback (an {@code OnBackInvokedCallback}), once registered. */
    private Object backCallback;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        // Read before the native thread starts (in super.onCreate), which may ask at once.
        String args = getIntent().getStringExtra("args");
        launchArgs = args != null ? args : "";
        super.onCreate(savedInstanceState);

        Window window = getWindow();
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        if (Build.VERSION.SDK_INT >= 28) {
            // Draw under cutouts on every edge; the engine keeps the view in the safe area.
            WindowManager.LayoutParams attributes = window.getAttributes();
            attributes.layoutInDisplayCutoutMode = Build.VERSION.SDK_INT >= 30
                    ? WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
                    : WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            window.setAttributes(attributes);
        }
        window.getDecorView().setOnApplyWindowInsetsListener((view, insets) -> {
            recordInsets(insets);
            return view.onApplyWindowInsets(insets);
        });
        if (Build.VERSION.SDK_INT >= 33) {
            backCallback = Api33.registerBack(this);
        }
        hideSystemBars();
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        // NativeActivity passes focus changes on to the game; keep that.
        super.onWindowFocusChanged(hasFocus);
        // The bars come back after a swipe, a dialog or another app; hide them again.
        if (hasFocus) {
            hideSystemBars();
        }
    }

    /** Android 12 and older (13 and newer use the callback): count it, stay open. */
    @Override
    @SuppressWarnings("deprecation")
    public void onBackPressed() {
        backPresses++;
    }

    @Override
    protected void onDestroy() {
        if (Build.VERSION.SDK_INT >= 33 && backCallback != null) {
            Api33.unregisterBack(this, backCallback);
        }
        super.onDestroy();
        // The game runs once per process: winit cannot start a second event loop in one
        // process, so an activity created later in this process could not run the game.
        // Ending the process makes the next launch a fresh start.
        Process.killProcess(Process.myPid());
    }

    private void recordInsets(WindowInsets insets) {
        long left = 0;
        long top = 0;
        long right = 0;
        long bottom = 0;
        if (Build.VERSION.SDK_INT >= 28) {
            DisplayCutout cutout = insets.getDisplayCutout();
            if (cutout != null) {
                left = cutout.getSafeInsetLeft();
                top = cutout.getSafeInsetTop();
                right = cutout.getSafeInsetRight();
                bottom = cutout.getSafeInsetBottom();
            }
        }
        safeInsets = (left & 0xffff)
                | (top & 0xffff) << 16
                | (right & 0xffff) << 32
                | (bottom & 0xffff) << 48;
    }

    @SuppressWarnings("deprecation")
    private void hideSystemBars() {
        Window window = getWindow();
        if (Build.VERSION.SDK_INT >= 30) {
            window.setDecorFitsSystemWindows(false);
            WindowInsetsController controller = window.getInsetsController();
            if (controller != null) {
                controller.hide(WindowInsets.Type.systemBars());
                controller.setSystemBarsBehavior(
                        WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            }
        } else {
            window.getDecorView().setSystemUiVisibility(
                    View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                            | View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                            | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_FULLSCREEN);
        }
    }

    /** Android 13+ APIs, kept apart so older versions never load these classes. */
    private static final class Api33 {
        static Object registerBack(CashGearActivity activity) {
            OnBackInvokedCallback callback = () -> activity.backPresses++;
            activity.getOnBackInvokedDispatcher().registerOnBackInvokedCallback(
                    OnBackInvokedDispatcher.PRIORITY_DEFAULT, callback);
            return callback;
        }

        static void unregisterBack(CashGearActivity activity, Object callback) {
            activity.getOnBackInvokedDispatcher()
                    .unregisterOnBackInvokedCallback((OnBackInvokedCallback) callback);
        }
    }
}
