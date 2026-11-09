# context-capsule — windows: release build of the Windows recorder (target\release\context-capsule.exe).
# .\build.ps1            with voice notes (needs CMake, the MSVC C++ build tools and LLVM for whisper.cpp)
# .\build.ps1 -NoVoice   without voice notes (Rust + MSVC only)
param([switch]$NoVoice)
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..") # the capture-rs workspace

# The Claude extension and the any-AI program go inside the exe (build.rs) and next to it: both must be built first.
$mcpb = Join-Path (Get-Location) "..\mcp\dist-ext\context-capsule.mcpb"
$mcp = Join-Path (Get-Location) "..\mcp\dist-bin\context-capsule-mcp-windows-x64.exe"
if (-not (Test-Path $mcpb)) { Write-Error "$mcpb is missing: build it first with mcp/extension/build.sh"; exit 1 }
if (-not (Test-Path $mcp)) { Write-Error "$mcp is missing: build it first with mcp/build-binary.sh windows-x64"; exit 1 }

$cargoArgs = @("build", "--release", "-p", "context-capsule-windows")
if ($NoVoice) {
    $cargoArgs += "--no-default-features"
} else {
    foreach ($tool in @("cmake", "clang")) {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
            Write-Warning "$tool not found: whisper.cpp may not build. Install it, or run .\build.ps1 -NoVoice"
        }
    }
}

# There is no `nice` on Windows; run cargo below normal priority instead.
$cargo = Start-Process cargo -ArgumentList $cargoArgs -NoNewWindow -PassThru
try { $cargo.PriorityClass = "BelowNormal" } catch {}
$cargo.WaitForExit()
if ($cargo.ExitCode -ne 0) { exit $cargo.ExitCode }

$exe = Join-Path (Get-Location) "target\release\context-capsule.exe"
# Also next to the exe ("Connect Claude" opens the extension there; the exe writes both there itself when missing).
Copy-Item $mcpb (Split-Path $exe) -Force
Copy-Item $mcp (Join-Path (Split-Path $exe) "context-capsule-mcp.exe") -Force
Write-Host "Copied context-capsule.mcpb and context-capsule-mcp.exe next to the exe"
Write-Host "Built $exe"
