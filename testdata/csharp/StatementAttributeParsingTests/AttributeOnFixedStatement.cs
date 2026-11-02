
class C
{
    unsafe void Goo(int[] vals)
    {
        [A]fixed (int* p = vals) { }
    }
}