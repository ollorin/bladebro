export BLADE_HEADLESS=1 BLADE_RECORD=1
bladebro nav http://127.0.0.1:8765/mark.html
for t in t01-blue t02-green; do
  bladebro state record-start "$t"
  bladebro act click "turn ${t#*-}"
  bladebro state record-stop --value "PASS: $t"
done
bladebro stop
