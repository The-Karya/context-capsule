# context-capsule — ci: the Windows smoke test (windows-smoke.yml) — runs the real app on a GitHub Windows runner
# and takes screenshots. -Part shipped: the released exe (install, first run, second copy, uninstall).
# -Part pages: every page of the window (ui/ in WebView2) from -Exe, light and dark, with seeded data
# (windows-smoke-seed.py), at a few sizes, with Mica on Windows 11 and transparency effects off and on.
# -Part update: install the released exe (-Exe, e.g. 0.2.0), update it to -New (built from this commit) the way the
# app does, then uninstall: one Start menu shortcut and one Settings → Apps entry, both with the new name.
param(
    [Parameter(Mandatory)] [ValidateSet('shipped', 'pages', 'update')] [string] $Part,
    [Parameter(Mandatory)] [string] $Exe,
    [Parameter(Mandatory)] [string] $Out,
    [string] $New
)
$ErrorActionPreference = 'Continue'
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Summary = Join-Path $Out 'summary.txt'
function Note($s) { Write-Host $s; Add-Content -Path $Summary -Value $s }

Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class W {
    public delegate bool EnumProc(IntPtr h, IntPtr p);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr p);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr h, EnumProc f, IntPtr p);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hh, bool r);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, int d, IntPtr e);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    static string Cls(IntPtr h) { var s = new StringBuilder(256); GetClassName(h, s, 256); return s.ToString(); }
    static string Txt(IntPtr h) { var s = new StringBuilder(512); GetWindowText(h, s, 512); return s.ToString(); }
    // Every visible dialog (#32770): its title, its static texts and its buttons.
    public static string Dialogs() {
        var sb = new StringBuilder();
        EnumWindows((h, p) => {
            if (IsWindowVisible(h) && Cls(h) == "#32770") {
                sb.Append("[" + Txt(h) + "]");
                EnumChildWindows(h, (c, q) => { sb.Append(" <" + Cls(c) + ": " + Txt(c).Replace("\n", " / ") + ">"); return true; }, IntPtr.Zero);
                sb.Append("\n");
            }
            return true;
        }, IntPtr.Zero);
        return sb.ToString();
    }
    // Click the button with this text (without &) in any visible dialog. true: clicked.
    public static bool Click(string text) {
        bool done = false;
        EnumWindows((h, p) => {
            if (done || !IsWindowVisible(h) || Cls(h) != "#32770") return true;
            EnumChildWindows(h, (c, q) => {
                if (!done && Cls(c) == "Button" && Txt(c).Replace("&", "") == text) {
                    SendMessage(c, 0x00F5, IntPtr.Zero, IntPtr.Zero); // BM_CLICK
                    done = true;
                }
                return true;
            }, IntPtr.Zero);
            return true;
        }, IntPtr.Zero);
        return done;
    }
}
'@

# Screenshots that stayed blank (one colour all over, e.g. all black) after every try: the run fails at the end.
$BlankShots = @()
$ShotTries = 3

function Shot($name) {
    Start-Sleep -Milliseconds 400
    $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
    for ($try = 1; $try -le $ShotTries; $try++) {
        $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        try { $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size) } catch { Note "shot $name failed: $_" }
        # A 96 × 72 grid (fine enough to land on the taskbar): blank when every channel stays within 6 levels.
        $sum = 0; $n = 0; $lo = @(255, 255, 255); $hi = @(0, 0, 0)
        for ($x = 5; $x -lt $b.Width; $x += [Math]::Max(1, [int]($b.Width / 96))) {
            for ($y = 5; $y -lt $b.Height; $y += [Math]::Max(1, [int]($b.Height / 72))) {
                $p = $bmp.GetPixel($x, $y); $c = @($p.R, $p.G, $p.B)
                for ($i = 0; $i -lt 3; $i++) { if ($c[$i] -lt $lo[$i]) { $lo[$i] = $c[$i] }; if ($c[$i] -gt $hi[$i]) { $hi[$i] = $c[$i] } }
                $sum += ($p.R + $p.G + $p.B) / 3; $n++
            }
        }
        $blank = ($hi[0] - $lo[0] -le 6) -and ($hi[1] - $lo[1] -le 6) -and ($hi[2] - $lo[2] -le 6)
        if (-not $blank -or $try -eq $ShotTries) { $bmp.Save((Join-Path $Out "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png) }
        $g.Dispose(); $bmp.Dispose()
        if (-not $blank) { break }
        Note "shot ${name}: picture $try of $ShotTries is blank (one colour)"
        Start-Sleep -Seconds 2
    }
    if ($blank) { $script:BlankShots += $name }
    Note ("shot {0}: {1}x{2}, mean brightness {3:N0}{4}" -f $name, $b.Width, $b.Height, ($sum / $n), $(if ($blank) { " (BLANK after $ShotTries tries)" } else { '' }))
}

function WaitFor([scriptblock] $cond, [int] $seconds = 20) {
    $until = (Get-Date).AddSeconds($seconds)
    while ((Get-Date) -lt $until) { if (& $cond) { return $true }; Start-Sleep -Milliseconds 250 }
    return $false
}

function Click($text, [int] $seconds = 20) {
    $ok = WaitFor { [W]::Click($text) } $seconds
    Note "click '$text': $ok"
    return $ok
}

function Dialogs { $d = [W]::Dialogs(); if ($d) { Note "dialogs: $d" } }
function Running { @(Get-Process -Name 'context-capsule' -ErrorAction SilentlyContinue) }
function WebWindow { [W]::FindWindow('ContextCapsuleWebWindow', [NullString]::Value) }
# The window of a released exe: releases before 0.3.0 opened the custom-drawn one (class ContextCapsuleWindow).
function ReleasedWindow {
    $h = WebWindow
    if ($h -ne [IntPtr]::Zero) { return $h }
    [W]::FindWindow('ContextCapsuleWindow', [NullString]::Value)
}
function StopApp {
    Running | Stop-Process -Force -ErrorAction SilentlyContinue
    WaitFor { (Running).Count -eq 0 } 10 | Out-Null
    Start-Sleep -Milliseconds 300
}

$Data = Join-Path $env:USERPROFILE '.context-capsule'
$Dir = Join-Path $env:LOCALAPPDATA 'Programs\Context Capsule'
$Installed = Join-Path $Dir 'context-capsule.exe'
$StartMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
$UninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ContextCapsule'
# Up to 0.2.0 the shortcut was "Context Capsule.lnk"; from 0.2.1 "Karya Context Capsule.lnk".
function Shortcuts { "old $(Test-Path (Join-Path $StartMenu 'Context Capsule.lnk')), new $(Test-Path (Join-Path $StartMenu 'Karya Context Capsule.lnk'))" }
function AppsEntries {
    $all = @(Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall' -ErrorAction SilentlyContinue | Get-ItemProperty -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -like '*Context Capsule*' })
    "$($all.Count) ($(($all | ForEach-Object { $_.DisplayName + ' ' + $_.DisplayVersion }) -join '; '))"
}
Note "screen: $([System.Windows.Forms.SystemInformation]::VirtualScreen)  dpi scale: $([System.Windows.Forms.Screen]::PrimaryScreen.Bounds)"
Shot '00-desktop'

if ($Part -eq 'shipped') {
    # 1. First launch: asks to install; Install copies it and starts the installed copy.
    $p = Start-Process -FilePath $Exe -PassThru
    if (WaitFor { [W]::Dialogs() -match 'Install' } 30) { Dialogs; Shot '01-install-question' } else { Note 'no install question'; Shot '01-no-question' }
    Click 'Install' | Out-Null
    $started = WaitFor { (Running | Where-Object { $_.Path -eq $Installed }).Count -ge 1 } 60
    Note "installed copy running: $started; exe: $(Test-Path $Installed); files: $((Get-ChildItem $Dir -ErrorAction SilentlyContinue).Name -join ', ')"
    Note "start menu shortcut: $(Shortcuts)"
    Note "uninstall entry: $(Test-Path $UninstallKey); Settings → Apps entries: $(AppsEntries)"
    $win = WaitFor { (ReleasedWindow) -ne [IntPtr]::Zero } 20
    Note "first-run window open: $win"
    Start-Sleep -Seconds 4
    Dialogs
    Shot '02-first-run'

    # 2. A second launch: no second copy.
    $before = (Running).Count
    Start-Process -FilePath $Installed
    if (WaitFor { [W]::Dialogs() -match 'already running' } 15) { Dialogs; Shot '03-second-launch' } else { Note 'no "already running" message'; Shot '03-second-launch' }
    Click 'OK' 5 | Out-Null
    Start-Sleep -Seconds 2
    Note "copies before: $before, after second launch: $((Running).Count)"

    # 3. Uninstall (keep the recordings: No).
    Start-Process -FilePath $Installed -ArgumentList '--uninstall'
    if (WaitFor { [W]::Dialogs() -match 'delete' } 20) { Dialogs; Shot '04-uninstall-question' } else { Note 'no delete question'; Dialogs; Shot '04-uninstall' }
    Click 'No' | Out-Null
    if (WaitFor { [W]::Dialogs() -match 'removed' } 20) { Dialogs; Shot '05-removed' } else { Note 'no removed message'; Dialogs; Shot '05-after-uninstall' }
    Click 'OK' 5 | Out-Null
    WaitFor { -not (Test-Path $Installed) } 20 | Out-Null
    Start-Sleep -Seconds 3
    Note "after uninstall: running $((Running).Count), folder $(Test-Path $Dir), shortcut $(Shortcuts), uninstall entry $(Test-Path $UninstallKey), data kept $(Test-Path $Data)"
    Shot '06-after-uninstall'
    StopApp
} elseif ($Part -eq 'update') {
    # 1. Install the released version.
    Start-Process -FilePath $Exe
    if (WaitFor { [W]::Dialogs() -match 'Install' } 30) { Dialogs; Shot 'u01-install-question' } else { Note 'no install question'; Shot 'u01-no-question' }
    Click 'Install' | Out-Null
    $started = WaitFor { (Running | Where-Object { $_.Path -eq $Installed }).Count -ge 1 } 60
    Start-Sleep -Seconds 3
    Note "released version installed: $started; shortcuts: $(Shortcuts); Settings → Apps: $(AppsEntries)"
    # 2. Update the way the app does: the running exe moves aside, the new one moves in and starts with --updated.
    StopApp
    Move-Item $Installed "$Installed.old" -Force
    Copy-Item $New $Installed -Force
    Start-Process -FilePath $Installed -ArgumentList '--updated', '0'
    $updated = WaitFor { (Running | Where-Object { $_.Path -eq $Installed }).Count -ge 1 } 30
    WaitFor { -not (Test-Path (Join-Path $StartMenu 'Context Capsule.lnk')) } 20 | Out-Null
    Start-Sleep -Seconds 4
    Dialogs
    Note "updated copy running: $updated; leftover .old: $(Test-Path "$Installed.old")"
    Note "after update: shortcuts: $(Shortcuts) (want: old False, new True)"
    Note "after update: Settings → Apps: $(AppsEntries) (want: 1, Karya Context Capsule)"
    $lnk = (New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path $StartMenu 'Karya Context Capsule.lnk'))
    Note "new shortcut points at: $($lnk.TargetPath)"
    Note "exe description: $((Get-Item $Installed).VersionInfo.FileDescription)"
    if ((WebWindow) -ne [IntPtr]::Zero) { $t = New-Object System.Text.StringBuilder 256; [W]::GetWindowText((WebWindow), $t, 256) | Out-Null; Note "window title: $t" }
    Shot 'u02-after-update'
    # 3. Uninstall: both shortcut names go, the entry goes.
    Start-Process -FilePath $Installed -ArgumentList '--uninstall'
    if (WaitFor { [W]::Dialogs() -match 'delete' } 20) { Dialogs; Shot 'u03-uninstall-question' } else { Note 'no delete question'; Dialogs }
    Click 'No' | Out-Null
    if (WaitFor { [W]::Dialogs() -match 'removed' } 20) { Dialogs; Shot 'u04-removed' } else { Note 'no removed message'; Dialogs }
    Click 'OK' 5 | Out-Null
    WaitFor { -not (Test-Path $Installed) } 20 | Out-Null
    Start-Sleep -Seconds 3
    Note "after uninstall: running $((Running).Count), folder $(Test-Path $Dir), shortcuts $(Shortcuts), Settings → Apps: $(AppsEntries)"
    StopApp
} else {
    # The build from main, put where the installer would (so it doesn't ask to install).
    New-Item -ItemType Directory -Force -Path $Dir | Out-Null
    Copy-Item $Exe $Installed -Force
    # One clock for the seed and the window: `--show <page> <day>` sets the page's clock to <day> 19:00
    # (show_query in web_window.rs), so the data is seeded for exactly that.
    $clock = (Get-Date).ToString('yyyy-MM-dd') + 'T19:00:00'
    $seed = @(python (Join-Path $PSScriptRoot 'windows-smoke-seed.py') $Data $clock)
    $day = $seed[0]; $session = $seed[1]
    Note "seeded $day for the clock $clock, first recording $session"
    $config = Join-Path $Data 'config.json'

    # Scrolls with the real wheel over the page.
    function WebPage($name, [string[]] $show, [scriptblock] $then) {
        StopApp
        if ($show) { Start-Process -FilePath $Installed -ArgumentList $show } else { Start-Process -FilePath $Installed }
        if (-not (WaitFor { (WebWindow) -ne [IntPtr]::Zero } 20)) { Note "${name}: no window"; Dialogs; Shot "$name-nowindow"; return }
        Start-Sleep -Seconds 4
        Dialogs
        Shot $name
        if ($then) { & $then }
    }
    function Wheel($notches) {
        $h = WebWindow; $r = New-Object W+RECT; [W]::GetWindowRect($h, [ref]$r) | Out-Null
        [W]::SetForegroundWindow($h) | Out-Null
        [W]::SetCursorPos([int](($r.Left + $r.Right) / 2 + 100), [int](($r.Top + $r.Bottom) / 2)) | Out-Null
        for ($i = 0; $i -lt $notches; $i++) { [W]::mouse_event(0x0800, 0, 0, -120, [IntPtr]::Zero); Start-Sleep -Milliseconds 40 }
        Start-Sleep -Milliseconds 800
    }
    function WebSize($w, $h) { [W]::MoveWindow((WebWindow), 0, 0, $w, $h, $true) | Out-Null; Start-Sleep -Milliseconds 1200 }
    Remove-ItemProperty -Path 'HKCU:\Software\Context Capsule' -Name 'SetupShown' -ErrorAction SilentlyContinue
    Set-Content -Path $config -Value '{"author":"Saachi"}' -Encoding utf8NoBOM
    WebPage 'w00-first-run' @()
    foreach ($look in 'light', 'dark') {
        Set-Content -Path $config -Value ('{"author":"Saachi","appearance":"' + $look + '"}') -Encoding utf8NoBOM
        WebPage "w01-home-$look" @('--show', 'home', $day) { Wheel 10; Shot "w01-home-$look-scrolled" }
        WebPage "w02-timeline-$look" @('--show', 'timeline', $day)
        WebPage "w03-timeline-open-$look" @('--show', 'timeline', $day, $session) { Wheel 6; Shot "w03-timeline-open-$look-scrolled" }
        WebPage "w04-capsules-$look" @('--show', 'capsules', $day)
        WebPage "w05-projects-$look" @('--show', 'projects', $day)
        WebPage "w06-settings-$look" @('--show', 'settings', $day) { Wheel 8; Shot "w06-settings-$look-scrolled"; Wheel 12; Shot "w06-settings-$look-scrolled2" }
        WebPage "w07-help-$look" @('--show', 'help', $day) { Wheel 10; Shot "w07-help-$look-scrolled" }
    }
    Set-Content -Path $config -Value '{"author":"Saachi","appearance":"light"}' -Encoding utf8NoBOM
    WebPage 'w10-timeline-sizes' @('--show', 'timeline', $day, $session) { WebSize 640 520; Shot 'w10-timeline-small'; WebSize 1900 1040; Shot 'w10-timeline-large' }
    # Transparency effects off: flat (material "none"); on again: glass, live.
    WebPage 'w11-home-transparency' @('--show', 'home', $day) {
        Shot 'w11-home-glass'
        $pk = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize'
        if (-not (Test-Path $pk)) { New-Item -Path $pk | Out-Null }
        Set-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' -Name 'EnableTransparency' -Value 0 -Type DWord
        Add-Type -Namespace N -Name B -MemberDefinition '[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr h, uint m, IntPtr w, string l, uint f, uint t, out IntPtr r);'
        $r = [IntPtr]::Zero; [N.B]::SendMessageTimeout([IntPtr]0xffff, 0x001A, [IntPtr]::Zero, 'ImmersiveColorSet', 2, 3000, [ref]$r) | Out-Null
        Start-Sleep -Seconds 2; Shot 'w11-home-flat'
        Set-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' -Name 'EnableTransparency' -Value 1 -Type DWord
        [N.B]::SendMessageTimeout([IntPtr]0xffff, 0x001A, [IntPtr]::Zero, 'ImmersiveColorSet', 2, 3000, [ref]$r) | Out-Null
        Start-Sleep -Seconds 2
    }
    Note "transparency effects: $((Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' -ErrorAction SilentlyContinue).EnableTransparency)"
    Note "still running at the end: $((Running).Count)"
    StopApp
}

Copy-Item (Join-Path $Data 'app.log') (Join-Path $Out 'app.log') -ErrorAction SilentlyContinue
$log = Get-Content (Join-Path $Data 'app.log') -ErrorAction SilentlyContinue
Note "app.log lines: $($log.Count); error-like lines:"
$log | Select-String -Pattern 'error|fail|panic|cannot|couldn' | ForEach-Object { Note "  $_" }

if ($BlankShots.Count) {
    Note "FAILED: $($BlankShots.Count) screenshot(s) stayed blank (one colour) after $ShotTries tries: $($BlankShots -join ', ')"
    exit 1
}
