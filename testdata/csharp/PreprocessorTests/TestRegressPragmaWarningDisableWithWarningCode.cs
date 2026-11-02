
class A
{
    static void Main(int i)
    {
#pragma warning disable 1633
#pragma something // warning CS1633: Unrecognized #pragma directive
    }
}
