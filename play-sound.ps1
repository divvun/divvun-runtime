 param (
    [string]$file
 )

 (New-Object Media.SoundPlayer "$file").PlaySync()