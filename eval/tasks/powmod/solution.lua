local function powmod(base, exponent, modulus)
  local result, b, e = 1 % modulus, base % modulus, exponent
  while e > 0 do
    if e % 2 == 1 then result = result * b % modulus end
    b = b * b % modulus
    e = e // 2
  end
  return result
end
print(powmod(2, 10, 1000))
print(powmod(3, 200, 40009))
print(powmod(7, 123456, 30011))
print(powmod(10, 0, 7))
