public struct file { public int item; }

public unsafe class C
{
    public file _file;
    public file[] _array;
    public file* _ptr;
    public file? _nullable;
    public delegate*<file, file> _funcPtr;
    public (file, file) _tuple;
}