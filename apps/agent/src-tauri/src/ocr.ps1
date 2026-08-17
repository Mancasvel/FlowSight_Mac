param(
    [string]$ImagePath
)

$ErrorActionPreference = "Stop"

# Load Windows Runtime types (Use $null assignment to avoid void parsing issues)
$null = [Windows.Globalization.Language, Windows.Foundation.UniversalApiContract, ContentType = WindowsRuntime]
$null = [Windows.Media.Ocr.OcrEngine, Windows.Foundation.UniversalApiContract, ContentType = WindowsRuntime]
$null = [Windows.Graphics.Imaging.BitmapDecoder, Windows.Foundation.UniversalApiContract, ContentType = WindowsRuntime]
$null = [Windows.Storage.StorageFile, Windows.Foundation.UniversalApiContract, ContentType = WindowsRuntime]
$null = [System.Uri, System.Runtime, ContentType = WindowsRuntime]

try {
    # Get file asynchronously properly waiting for results
    $asTaskGeneric = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where { $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' })[0]
    
    # Get File
    $path = [System.IO.Path]::GetFullPath($ImagePath)
    $fileOp = [Windows.Storage.StorageFile]::GetFileFromPathAsync($path)
    $fileTask = $asTaskGeneric.MakeGenericMethod([Windows.Storage.StorageFile]).Invoke($null, @($fileOp))
    $file = $fileTask.Result
    
    # Open Stream
    $streamOp = $file.OpenAsync([Windows.Storage.FileAccessMode]::Read)
    $streamTask = $asTaskGeneric.MakeGenericMethod([Windows.Storage.Streams.IRandomAccessStream]).Invoke($null, @($streamOp))
    $stream = $streamTask.Result
    
    # Decode Image
    $decoderOp = [Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream)
    $decoderTask = $asTaskGeneric.MakeGenericMethod([Windows.Graphics.Imaging.BitmapDecoder]).Invoke($null, @($decoderOp))
    $decoder = $decoderTask.Result
    
    # Get SoftwareBitmap
    $bmpOp = $decoder.GetSoftwareBitmapAsync()
    $bmpTask = $asTaskGeneric.MakeGenericMethod([Windows.Graphics.Imaging.SoftwareBitmap]).Invoke($null, @($bmpOp))
    $bitmap = $bmpTask.Result
    
    # Init OCR Engine
    $lang = [Windows.Globalization.Language]::new("en-US")
    $engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromLanguage($lang)
    
    if ($null -eq $engine) {
        $engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages()
    }
    
    if ($null -eq $engine) {
        Write-Output "OCR_ERROR: Could not create OCR Engine"
        exit 1
    }
    
    # Recognize
    $ocrOp = $engine.RecognizeAsync($bitmap)
    $ocrTask = $asTaskGeneric.MakeGenericMethod([Windows.Media.Ocr.OcrResult]).Invoke($null, @($ocrOp))
    $result = $ocrTask.Result
    
    # Output Lines
    foreach ($line in $result.Lines) {
        Write-Output $line.Text
    }
    
} catch {
    Write-Output "OCR_ERROR: $_"
    exit 1
}
