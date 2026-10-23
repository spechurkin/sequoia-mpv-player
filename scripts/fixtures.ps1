$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force artifacts/fixtures | Out-Null
& ffmpeg -hide_banner -loglevel error -y -f lavfi -i 'testsrc2=size=640x360:rate=25' -f lavfi -i 'sine=frequency=220:sample_rate=44100' -t 8 -c:v libx264 -pix_fmt yuv420p -c:a aac artifacts/fixtures/test.mp4
if ($LASTEXITCODE)
{
    throw 'Video fixture generation failed'
}
& ffmpeg -hide_banner -loglevel error -y -i artifacts/fixtures/test.mp4 -t 4 -vf 'fps=10,scale=320:-1' artifacts/fixtures/test.gif
& ffmpeg -hide_banner -loglevel error -y -f lavfi -i 'sine=frequency=440:sample_rate=44100' -t 8 -metadata title='Тестовый сигнал' -metadata artist='Тестовая запись' artifacts/fixtures/test.mp3
& ffmpeg -hide_banner -loglevel error -y -i artifacts/fixtures/test.mp3 artifacts/fixtures/test.flac
& ffmpeg -hide_banner -loglevel error -y -f lavfi -i 'color=c=0xDDB177:s=128x128' -frames:v 1 artifacts/fixtures/cover.png
if ($LASTEXITCODE)
{
    throw 'Audio/art fixture generation failed'
}
@'
1
00:00:00,200 --> 00:00:05,000
Sequoia subtitle test
'@ | Set-Content -Encoding utf8 artifacts/fixtures/test.srt
& ffmpeg -hide_banner -loglevel error -y -i artifacts/fixtures/test.mp4 -i artifacts/fixtures/test.srt -map 0:v -map 0:v -map 0:a -map 0:a -map 1:s -c copy -metadata:s:a:0 language=rus -metadata:s:a:1 language=eng artifacts/fixtures/multi.mkv
if ($LASTEXITCODE)
{
    throw 'Multitrack fixture generation failed'
}
