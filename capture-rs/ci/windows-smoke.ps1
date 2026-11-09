# context-capsule — ci: the Windows smoke test (windows-smoke.yml) — runs the real app on a GitHub Windows runner
# and takes screenshots. -Part shipped: the released exe (install, first run, second copy, uninstall).
# -Part pages: every page of the window (ui/ in WebView2) from -Exe, light and dark, with seeded data
# (windows-smoke-seed.py), at a few sizes, with Mica on Windows 11 and transparency effects off and on.
# -Part update: install the released exe (-Exe, e.g. 0.2.0), update it to -New (built from this commit) the way the
# app does, then uninstall: one Start menu shortcut and one Settings → Apps entry, both with the new name; a recording and
# a capsule made before the update are still there, unchanged, and shown after it; the helper and Claude extension next
# to the exe are this commit's (-Helper, -Extension: the files the build put inside the exe). Not tested here: the signed
# download (latest.json, its signature, size and SHA-256), which needs a signed feed of its own.
# Pages wait until the window shows the page's words (UI Automation) before the picture. A check that fails is
# "FAILED: …" in summary.txt, and the script exits 1 at the end (after every picture).
param(
    [Parameter(Mandatory)] [ValidateSet('shipped', 'pages', 'update')] [string] $Part,
    [Parameter(Mandatory)] [string] $Exe,
    [Parameter(Mandatory)] [string] $Out,
    [string] $New,
    [string] $Helper,
    [string] $Extension
)
$ErrorActionPreference = 'Continue'
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Summary = Join-Path $Out 'summary.txt'
function Note($s) { Write-Host $s; Add-Content -Path $Summary -Value $s }
$script:Failed = @()
function Check([bool] $ok, [string] $what) {
    if ($ok) { Note "ok: $what" } else { Note "FAILED: $what"; $script:Failed += $what }
}

Add-Type -AssemblyName System.Drawing, System.Windows.Forms, UIAutomationClient, UIAutomationTypes
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

function Shot($name) {
    Start-Sleep -Milliseconds 400
    $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    try { $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size) } catch { Note "shot $name failed: $_" }
    $sum = 0; $n = 0
    for ($x = 5; $x -lt $b.Width; $x += [int]($b.Width / 24)) {
        for ($y = 5; $y -lt $b.Height; $y += [int]($b.Height / 18)) { $p = $bmp.GetPixel($x, $y); $sum += ($p.R + $p.G + $p.B) / 3; $n++ }
    }
    $bmp.Save((Join-Path $Out "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Note ("shot {0}: {1}x{2}, mean brightness {3:N0}{4}" -f $name, $b.Width, $b.Height, ($sum / $n), $(if ($sum / $n -lt 3) { ' (BLACK?)' } else { '' }))
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
# The words the window shows, through UI Automation (WebView2 hands the page's text to it). The page draws its first
# view once its reads are done (ui/ sets data-ready then): until then, or when it's blank, its words aren't there.
function PageText {
    $h = WebWindow
    if ($h -eq [IntPtr]::Zero) { return '' }
    try {
        $all = [System.Windows.Automation.AutomationElement]::FromHandle($h).FindAll(
            [System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
        $names = foreach ($e in $all) { $e.Current.Name }
        return ($names -join "`n")
    } catch { return '' }
}
function MissingWords([string[]] $words) { $t = PageText; @($words | Where-Object { -not $t.Contains($_) }) }
# Waits until the window shows every one of the words, and checks it did.
function PageShows([string] $name, [string[]] $words, [int] $seconds = 45) {
    $ok = WaitFor { (MissingWords $words).Count -eq 0 } $seconds
    $why = if ($ok) { '' } else { " (missing: $((MissingWords $words) -join ', '))" }
    Check $ok "${name}: the page shows $($words -join ', ')$why"
}
function Sha($path) { if ($path -and (Test-Path -LiteralPath $path)) { (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash } else { '' } }
# The helper and the Claude extension next to the exe are this commit's (the exe writes them at start, the helper on a
# thread): proof the build carried them, and that an older install's copies don't stand in for them.
function AiPartsCurrent([string] $name) {
    $h = Sha $Helper; $x = Sha $Extension
    Check ($h -ne '' -and $x -ne '') "${name}: this commit's helper and extension given (-Helper $Helper, -Extension $Extension)"
    $ok = WaitFor { (Sha (Join-Path $Dir 'context-capsule-mcp.exe')) -eq $h -and (Sha (Join-Path $Dir 'context-capsule.mcpb')) -eq $x } 60
    Check ($ok -and $h -ne '') "${name}: context-capsule-mcp.exe and context-capsule.mcpb next to the exe are this commit's"
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
    Check $started 'install: the installed copy runs'
    # The first run opens the window on Settings by itself.
    $win = WaitFor { (WebWindow) -ne [IntPtr]::Zero } 30
    Check $win 'first run: the window opens'
    if ($win) { PageShows 'first run' @('Your name', 'Updates') }
    Start-Sleep -Seconds 1
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
    Check $started 'the released version installs and runs'
    StopApp
    # A recording and a capsule made with the released version (today: old recordings get deleted after 30 days).
    $now = (Get-Date).ToUniversalTime()
    $sid = $now.AddMinutes(-30).ToString('yyyyMMdd-HHmmss')
    $z = { param($t) $t.ToString("yyyy-MM-dd'T'HH:mm:ss'Z'") }
    $rec = Join-Path $Data "sessions\$sid"
    New-Item -ItemType Directory -Force -Path $rec, (Join-Path $Data 'capsules') | Out-Null
    $sentinelTitle = "Smoke sentinel $sid"
    $ev = [ordered]@{ t = (& $z $now.AddMinutes(-30)); end = (& $z $now.AddMinutes(-29)); src = 'ax'; app = 'Notepad'; bundle = 'notepad.exe'; title = 'sentinel.txt - Notepad'; text = "$sentinelTitle`nThis line must survive the update." }
    Set-Content -Path (Join-Path $rec 'events.jsonl') -Value ($ev | ConvertTo-Json -Compress) -Encoding utf8NoBOM
    Set-Content -Path (Join-Path $rec 'session.json') -Value (@{ id = $sid; started_at = (& $z $now.AddMinutes(-30)); ended_at = (& $z $now.AddMinutes(-29)); host = 'windows'; version = 1 } | ConvertTo-Json -Compress) -Encoding utf8NoBOM
    $capsule = Join-Path $Data 'capsules\smoke-sentinel.capsule.md'
    Set-Content -Path $capsule -Encoding utf8NoBOM -Value @"
---
capsule: 1
title: $sentinelTitle
author: Smoke
created: $(& $z $now)
sessions: [$sid]
apps: [Notepad]
---

## In short
A capsule made before the update; it must still be here, unchanged, after it.
"@
    $sentinel = @((Join-Path $rec 'events.jsonl'), (Join-Path $rec 'session.json'), $capsule)
    $before = $sentinel | ForEach-Object { Sha $_ }
    Note "sentinel: recording $sid, capsule $capsule"
    # 2. Update the way the app does: the running exe moves aside, the new one moves in and starts with --updated.
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
    Check $updated 'update: the new version runs'
    Check (-not (Test-Path "$Installed.old")) 'update: the old exe (.old) is gone'
    Check ((Shortcuts) -eq 'old False, new True') 'update: one Start menu shortcut, the new name'
    AiPartsCurrent 'update'
    $after = $sentinel | ForEach-Object { Sha $_ }
    Check (($after -join ',') -eq ($before -join ',') -and -not ($after -contains '')) 'update: the recording and the capsule made before it are still there, unchanged'
    # The updated app reads them: the capsule is on the Capsules page, the recording on that day's Timeline.
    StopApp
    Start-Process -FilePath $Installed -ArgumentList '--show', 'capsules'
    if (WaitFor { (WebWindow) -ne [IntPtr]::Zero } 20) { PageShows 'update: Capsules' @($sentinelTitle) } else { Check $false 'update: the window opens on Capsules' }
    Shot 'u02-capsules-after-update'
    StopApp
    Start-Process -FilePath $Installed -ArgumentList '--show', 'timeline', $now.ToLocalTime().AddMinutes(-30).ToString('yyyy-MM-dd'), $sid
    if (WaitFor { (WebWindow) -ne [IntPtr]::Zero } 20) { PageShows 'update: Timeline' @('Show what was read') } else { Check $false 'update: the window opens on the Timeline' }
    Shot 'u02-timeline-after-update'
    # 3. Uninstall: both shortcut names go, the entry goes.
    Start-Process -FilePath $Installed -ArgumentList '--uninstall'
    if (WaitFor { [W]::Dialogs() -match 'delete' } 20) { Dialogs; Shot 'u03-uninstall-question' } else { Note 'no delete question'; Dialogs }
    Click 'No' | Out-Null
    if (WaitFor { [W]::Dialogs() -match 'removed' } 20) { Dialogs; Shot 'u04-removed' } else { Note 'no removed message'; Dialogs }
    Click 'OK' 5 | Out-Null
    WaitFor { -not (Test-Path $Installed) } 20 | Out-Null
    Start-Sleep -Seconds 3
    Note "after uninstall: running $((Running).Count), folder $(Test-Path $Dir), shortcuts $(Shortcuts), Settings → Apps: $(AppsEntries)"
    Check (-not (Test-Path $Dir)) 'uninstall: the program folder is gone'
    Check ((($sentinel | ForEach-Object { Sha $_ }) -join ',') -eq ($before -join ',')) 'uninstall (keep my recordings: No): the recording and the capsule are still there'
    StopApp
} else {
    # The build from main, put where the installer would (so it doesn't ask to install).
    New-Item -ItemType Directory -Force -Path $Dir | Out-Null
    Copy-Item $Exe $Installed -Force
    $seed = @(python (Join-Path $PSScriptRoot 'windows-smoke-seed.py') $Data)
    $day = $seed[0]; $session = $seed[1]
    Note "seeded $day, first recording $session"
    $config = Join-Path $Data 'config.json'

    # Opens the window (on a page with --show), waits for the page's words (from the seeded data), takes the picture.
    function WebPage($name, [string[]] $show, [string[]] $words, [scriptblock] $then) {
        StopApp
        if ($show) { Start-Process -FilePath $Installed -ArgumentList $show } else { Start-Process -FilePath $Installed }
        if (-not (WaitFor { (WebWindow) -ne [IntPtr]::Zero } 20)) { Check $false "${name}: the window opens"; Dialogs; Shot "$name-nowindow"; return }
        PageShows $name $words
        Start-Sleep -Seconds 1
        Dialogs
        Shot $name
        if ($then) { & $then }
    }
    # Scrolls with the real wheel over the page.
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
    # First run: the window opens by itself on Settings.
    WebPage 'w00-first-run' @() @('Your name', 'Updates')
    AiPartsCurrent 'this build'
    foreach ($look in 'light', 'dark') {
        Set-Content -Path $config -Value ('{"author":"Saachi","appearance":"' + $look + '"}') -Encoding utf8NoBOM
        WebPage "w01-home-$look" @('--show', 'home', $day) @('Saachi') { Wheel 10; Shot "w01-home-$look-scrolled" }
        WebPage "w02-timeline-$look" @('--show', 'timeline', $day) @('Website')
        WebPage "w03-timeline-open-$look" @('--show', 'timeline', $day, $session) @('Website', 'Show what was read') { Wheel 6; Shot "w03-timeline-open-$look-scrolled" }
        WebPage "w04-capsules-$look" @('--show', 'capsules', $day) @('Client report')
        WebPage "w05-projects-$look" @('--show', 'projects', $day) @('Client report')
        WebPage "w06-settings-$look" @('--show', 'settings', $day) @('Your name', 'Updates') { Wheel 8; Shot "w06-settings-$look-scrolled"; Wheel 12; Shot "w06-settings-$look-scrolled2" }
        WebPage "w07-help-$look" @('--show', 'help', $day) @('How it works') { Wheel 10; Shot "w07-help-$look-scrolled" }
    }
    Set-Content -Path $config -Value '{"author":"Saachi","appearance":"light"}' -Encoding utf8NoBOM
    WebPage 'w10-timeline-sizes' @('--show', 'timeline', $day, $session) @('Show what was read') { WebSize 640 520; Shot 'w10-timeline-small'; WebSize 1900 1040; Shot 'w10-timeline-large' }
    # Transparency effects off: flat (material "none"); on again: glass, live.
    WebPage 'w11-home-transparency' @('--show', 'home', $day) @('Saachi') {
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

# A failed check fails the job (each one is listed in summary.txt as FAILED: ...).
if ($script:Failed.Count) { Note "$($script:Failed.Count) check(s) failed"; exit 1 }
