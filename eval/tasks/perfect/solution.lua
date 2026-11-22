local function divisorSum(n)
  local sum = 0
  for d = 1, n - 1 do
    if n % d == 0 then sum = sum + d end
  end
  return sum
end
for n = 2, 9999 do
  if divisorSum(n) == n then print(n) end
end
local abundant = 0
for n = 1, 999 do
  if divisorSum(n) > n then abundant = abundant + 1 end
end
print(abundant)
