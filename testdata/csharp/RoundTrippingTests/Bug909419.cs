
class Test
{
static int Main()
{
  int n1 = test is Test ? 0 : 1;
  int n2 = null == test as Test ? 0 : 1;
  return n1 + n2;
}
}
