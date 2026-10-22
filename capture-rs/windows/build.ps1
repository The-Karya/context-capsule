# context-capsule — windows: release build of the Windows recorder (target\release\context-capsule.exe).
# .\build.ps1            with voice notes (needs CMake, the MSVC C++ build tools and LLVM for whisper.cpp)
# .\build.ps1 -NoVoice   without voice notes (Rust + MSVC only)
param([switch]$NoVoice)
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..") # the capture-rs workspace

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
# The Claude extension ships next to the exe ("Connect Claude" opens it). Built by mcp/extension/build.sh.
$mcpb = Join-Path (Get-Location) "..\mcp\dist-ext\context-capsule.mcpb"
if (Test-Path $mcpb) {
    Copy-Item $mcpb (Split-Path $exe) -Force
    Write-Host "Copied context-capsule.mcpb next to the exe"
} else {
    Write-Warning "$mcpb is missing: build it with mcp/extension/build.sh, or Connect Claude won't work"
}
# The any-AI program is inside the exe too (build.rs); built by mcp/build-binary.sh windows-x64.
$mcp = Join-Path (Get-Location) "..\mcp\dist-bin\context-capsule-mcp-windows-x64.exe"
if (Test-Path $mcp) {
    Copy-Item $mcp (Join-Path (Split-Path $exe) "context-capsule-mcp.exe") -Force
    Write-Host "Copied context-capsule-mcp.exe next to the exe"
} else {
    Write-Warning "$mcp is missing: build it with mcp/build-binary.sh windows-x64, or Connect your AI and Copy my day won't work"
}
Write-Host "Built $exe"
