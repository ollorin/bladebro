out=$(bladebro state record-stop --value "PASS: t03")
video=$(printf '%s\n' "$out" | sed -nE 's/^recording saved: (.+\.(webm|mp4)) \(bytes .*/\1/p')
poster=$(printf '%s\n' "$out" | sed -nE 's/.*\. Poster: (.+\.poster\.png)\..*/\1/p')
timeline=$(printf '%s\n' "$out" | sed -nE 's/.*\. Timeline: (.+\.timeline\.json) \(.*/\1/p')
echo "video: $video"; echo "poster: $poster"; echo "timeline: $timeline"
