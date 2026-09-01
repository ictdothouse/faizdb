param (
    [string]$Endpoint = "http://127.0.0.1:27018",
    [int]$Iterations = 50
)

Write-Host "==================================================================" -ForegroundColor Cyan
Write-Host "         FaizDB High-Velocity Performance Benchmark Suite         " -ForegroundColor Cyan
Write-Host "==================================================================" -ForegroundColor Cyan
Write-Host "  Target Endpoint : $Endpoint" -ForegroundColor Cyan
Write-Host "  Batch Iterations: $Iterations operations" -ForegroundColor Cyan
Write-Host "==================================================================" -ForegroundColor Cyan
Write-Host ""

# 1. Health check
Write-Host "-> Checking FaizDB engine health..." -ForegroundColor Yellow
try {
    $health = Invoke-RestMethod -Uri "$Endpoint/v1/health" -Method Get
    Write-Host "[OK] Engine Online: $($health.engine) v$($health.version)" -ForegroundColor Green
} catch {
    Write-Host "[ERROR] Failed to connect to FaizDB at $Endpoint" -ForegroundColor Red
    exit 1
}

# 2. Authentication
Write-Host "-> Authenticating with master JWT credentials..." -ForegroundColor Yellow
$loginBody = @{
    username = "admin"
    password = "faizdb-admin-2026"
} | ConvertTo-Json

$loginRes = Invoke-RestMethod -Uri "$Endpoint/v1/auth/login" -Method Post -ContentType "application/json" -Body $loginBody
$token = $loginRes.data.token
$headers = @{ Authorization = "Bearer $token" }
Write-Host "[OK] Authenticated as: $($loginRes.data.username)" -ForegroundColor Green
Write-Host ""

# 3. Stage 1: Document Ingestion
Write-Host "STAGE 1: Concurrent Document Ingestion ($Iterations records)..." -ForegroundColor Magenta
$sw = [System.Diagnostics.Stopwatch]::StartNew()

for ($i = 1; $i -le $Iterations; $i++) {
    $doc = @{
        benchmark_id = $i
        timestamp = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        metric_score = $i * 42.5
        active = $true
        tag = "stress_test"
    } | ConvertTo-Json

    $null = Invoke-RestMethod -Uri "$Endpoint/v1/collections/benchmarks/insert" -Method Post -ContentType "application/json" -Headers $headers -Body $doc
}

$sw.Stop()
$elapsedMs = [Math]::Max(1, $sw.ElapsedMilliseconds)
$opsSec = [Math]::Round(($Iterations * 1000) / $elapsedMs)
Write-Host "[OK] Ingestion Completed in $elapsedMs ms ($opsSec ops/sec)" -ForegroundColor Green
Write-Host ""

# 4. Stage 2: Okapi BM25 Fuzzy Search
$searchCount = [Math]::Max(10, [int]($Iterations / 2))
Write-Host "STAGE 2: Okapi BM25 Fuzzy Full-Text Search ($searchCount queries)..." -ForegroundColor Magenta
$sw.Restart()

for ($i = 1; $i -le $searchCount; $i++) {
    $searchBody = @{
        query = "stress test"
        fuzzy = $true
        top_k = 10
    } | ConvertTo-Json

    $null = Invoke-RestMethod -Uri "$Endpoint/v1/collections/benchmarks/search" -Method Post -ContentType "application/json" -Headers $headers -Body $searchBody
}

$sw.Stop()
$searchElapsedMs = [Math]::Max(1, $sw.ElapsedMilliseconds)
$searchQps = [Math]::Round(($searchCount * 1000) / $searchElapsedMs)
Write-Host "[OK] BM25 Search Completed in $searchElapsedMs ms ($searchQps QPS)" -ForegroundColor Green
Write-Host ""

Write-Host "Benchmark suite finished successfully!" -ForegroundColor Cyan
