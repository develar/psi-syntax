class Program
{
static void Main()
{
ulong x1 = 7L;
// ulong x2 = 7l; // should Warn
ulong x4 = 7Ul; // should NOT warn
ulong x6 = 7ul; // should NOT warn
}
}