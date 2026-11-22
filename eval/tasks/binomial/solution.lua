local c = 1
for k = 0, 20 do
  print(c)
  c = c * (20 - k) // (k + 1)
end
