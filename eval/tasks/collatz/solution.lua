local function steps(n)
  local count = 0
  while n ~= 1 do
    if n % 2 == 0 then n = n // 2 else n = 3 * n + 1 end
    count = count + 1
  end
  return count
end
for n = 1, 10 do print(steps(n)) end
local best, bestSteps = 1, 0
for n = 1, 9999 do
  local s = steps(n)
  if s > bestSteps then best, bestSteps = n, s end
end
print(best)
print(bestSteps)
