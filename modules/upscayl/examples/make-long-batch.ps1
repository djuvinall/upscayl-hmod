<#
.SYNOPSIS
Fill out/long-batch with copies of samples/sample-512.jpg for 07-long-batch-job.json.

.DESCRIPTION
80 copies with ultrasharp-4x and TTA run for roughly five to six minutes on an
RTX 5070 Ti (docs/results/perf-baseline.md: about 16.5 s per input megapixel with TTA),
which is past the 300 s tool timeout -- the point of the example. Raise -Count on a
faster GPU.
#>
param([int]$Count = 80)
$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$src = Join-Path $here 'samples\sample-512.jpg'
$dst = Join-Path $here 'out\long-batch'
New-Item -ItemType Directory -Force $dst | Out-Null
Get-ChildItem $dst -File | Remove-Item
1..$Count | ForEach-Object { Copy-Item $src (Join-Path $dst ('img{0:D3}.jpg' -f $_)) }
"{0} images in {1}" -f $Count, $dst
