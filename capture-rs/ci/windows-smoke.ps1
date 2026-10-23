# context-capsule — ci: the Windows smoke test (windows-smoke.yml) — runs the real app on a GitHub Windows runner
# and takes screenshots. -Part shipped: the released exe (install, first run, second copy, uninstall).
# -Part pages: every page of the window from -Exe, light and dark, with seeded data (windows-smoke-seed.py).
param(
    [Parameter(Mandatory)] [ValidateSet('shipped', 'pages')] [string] $Part,
    [Parameter(Mandatory)] [string] $Exe,
    [Parameter(Mandatory)] [string] $Out
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
function MainWindow { [W]::FindWindow('ContextCapsuleWindow', [NullString]::Value) }
function StopApp {
    Running | Stop-Process -Force -ErrorAction SilentlyContinue
    WaitFor { (Running).Count -eq 0 } 10 | Out-Null
    Start-Sleep -Milliseconds 300
}

$Data = Join-Path $env:USERPROFILE '.context-capsule'
$Dir = Join-Path $env:LOCALAPPDATA 'Programs\Context Capsule'
$Installed = Join-Path $Dir 'context-capsule.exe'
Note "screen: $([System.Windows.Forms.SystemInformation]::VirtualScreen)  dpi scale: $([System.Windows.Forms.Screen]::PrimaryScreen.Bounds)"
Shot '00-desktop'

if ($Part -eq 'shipped') {
    # 1. First launch: asks to install; Install copies it and starts the installed copy.
    $p = Start-Process -FilePath $Exe -PassThru
    if (WaitFor { [W]::Dialogs() -match 'Install' } 30) { Dialogs; Shot '01-install-question' } else { Note 'no install question'; Shot '01-no-question' }
    Click 'Install' | Out-Null
    $started = WaitFor { (Running | Where-Object { $_.Path -eq $Installed }).Count -ge 1 } 60
    Note "installed copy running: $started; exe: $(Test-Path $Installed); files: $((Get-ChildItem $Dir -ErrorAction SilentlyContinue).Name -join ', ')"
    Note "start menu shortcut: $(Test-Path (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Context Capsule.lnk'))"
    Note "uninstall entry: $(Test-Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ContextCapsule')"
    $win = WaitFor { (MainWindow) -ne [IntPtr]::Zero } 20
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
    Note "after uninstall: running $((Running).Count), folder $(Test-Path $Dir), shortcut $(Test-Path (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Context Capsule.lnk')), uninstall entry $(Test-Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ContextCapsule'), data kept $(Test-Path $Data)"
    Shot '06-after-uninstall'
    StopApp
} else {
    # The build from main, put where the installer would (so it doesn't ask to install).
    New-Item -ItemType Directory -Force -Path $Dir | Out-Null
    Copy-Item $Exe $Installed -Force
    $seed = @(python (Join-Path $PSScriptRoot 'windows-smoke-seed.py') $Data)
    $day = $seed[0]; $session = $seed[1]
    Note "seeded $day, first recording $session"
    $config = Join-Path $Data 'config.json'

    function Page($name, [string[]] $show, [scriptblock] $then) {
        StopApp
        if ($show) { Start-Process -FilePath $Installed -ArgumentList $show } else { Start-Process -FilePath $Installed }
        if (-not (WaitFor { (MainWindow) -ne [IntPtr]::Zero } 20)) { Note "${name}: no window"; Dialogs; Shot "$name-nowindow"; return }
        Start-Sleep -Seconds 3
        Dialogs
        Shot $name
        if ($then) { & $then }
    }
    function Scroll($notches) {
        $h = MainWindow; $r = New-Object W+RECT; [W]::GetWindowRect($h, [ref]$r) | Out-Null
        $cx = [int][math]::Floor(($r.Left + $r.Right) / 2); $cy = [int][math]::Floor(($r.Top + $r.Bottom) / 2)
        $lp = [IntPtr](($cy -shl 16) -bor $cx)
        for ($i = 0; $i -lt $notches; $i++) { [W]::PostMessage($h, 0x020A, [IntPtr]((-120 -band 0xFFFF) -shl 16), $lp) | Out-Null }
        Start-Sleep -Milliseconds 800
    }
    function Size($w, $h) { [W]::MoveWindow((MainWindow), 0, 0, $w, $h, $true) | Out-Null; Start-Sleep -Milliseconds 800 }

    # First run: no setup seen yet, opens on Settings.
    Page 'p00-first-run' $null { Scroll 8; Shot 'p00-first-run-scrolled' }

    foreach ($look in 'light', 'dark') {
        Set-Content -Path $config -Value ('{"author":"Saachi","appearance":"' + $look + '"}') -Encoding utf8NoBOM
        Page "p01-home-$look" @('--show', 'home') { Scroll 10; Shot "p01-home-$look-scrolled" }
        Page "p02-timeline-$look" @('--show', 'timeline', $day)
        Page "p03-timeline-open-$look" @('--show', 'timeline', $day, $session) { Scroll 6; Shot "p03-timeline-open-$look-scrolled"; Scroll 10; Shot "p03-timeline-open-$look-scrolled2" }
        Page "p04-capsules-$look" @('--show', 'capsules')
        Page "p05-projects-$look" @('--show', 'projects')
        Page "p06-settings-$look" @('--show', 'settings') { Scroll 8; Shot "p06-settings-$look-scrolled"; Scroll 12; Shot "p06-settings-$look-scrolled2" }
        Page "p07-help-$look" @('--show', 'help') { Scroll 10; Shot "p07-help-$look-scrolled" }
    }

    # Small and large windows (light).
    Set-Content -Path $config -Value '{"author":"Saachi","appearance":"light"}' -Encoding utf8NoBOM
    Page 'p10-timeline-small' @('--show', 'timeline', $day, $session) { Size 560 420; Shot 'p10-timeline-small'; Size 400 300; Shot 'p10-timeline-tiny'; Size 1900 1040; Shot 'p10-timeline-large' }
    Page 'p11-home-small' @('--show', 'home') { Size 560 420; Shot 'p11-home-small' }
    Page 'p12-settings-small' @('--show', 'settings') { Size 560 420; Shot 'p12-settings-small' }
    Page 'p13-capsules-small' @('--show', 'capsules') { Size 560 420; Shot 'p13-capsules-small' }
    # An empty day.
    Page 'p14-timeline-empty' @('--show', 'timeline', '2026-01-05')
    Note "still running at the end: $((Running).Count)"
    StopApp
}

Copy-Item (Join-Path $Data 'app.log') (Join-Path $Out 'app.log') -ErrorAction SilentlyContinue
$log = Get-Content (Join-Path $Data 'app.log') -ErrorAction SilentlyContinue
Note "app.log lines: $($log.Count); error-like lines:"
$log | Select-String -Pattern 'error|fail|panic|cannot|couldn' | ForEach-Object { Note "  $_" }
