#include "DShowSoftcam.h"

#include <cstring>
#include <string>
#include <algorithm>
#include <cstdio>
#include <cmath>
#include <cstdlib>
#include <chrono>
#include <ctime>


namespace {

//#define ENABLE_LOG
//#define LOG_FILE_PATH "C:\\my_temp\\debug_log.txt"

#if defined(ENABLE_LOG)

FILE* logfile = nullptr;
#define OPEN_LOGFILE() logfile = std::fopen(LOG_FILE_PATH, "a")
#define CLOSE_LOGFILE() [&]{ \
        if (logfile) std::fclose(logfile); \
        logfile = nullptr; \
    }()
#define NOW() []{ \
        auto tp = std::chrono::system_clock::now(); \
        auto t = std::chrono::system_clock::to_time_t(tp); \
        static char buff[128]; \
        std::strftime(buff, sizeof(buff), "%H:%M:%S", std::localtime(&t)); \
        return buff; \
    }()
std::string IID_TO_STR(REFIID riid)
{
    std::string s =
            riid == IID_IPin                ? "IPin" :
            riid == IID_IBaseFilter         ? "IBaseFilter" :
            riid == IID_IAMovieSetup        ? "IAMovieSetup" :
            riid == IID_IQualityControl     ? "IQualityControl" :
            riid == IID_IAMStreamConfig     ? "IAMStreamConfig" :
            riid == IID_IKsPropertySet      ? "IKsPropertySet" :
            riid == IID_IAMFilterMiscFlags  ? "IAMFilterMiscFlags" :
            riid == IID_IPersistPropertyBag ? "IPersistPropertyBag" :
            riid == IID_IReferenceClock     ? "IReferenceClock" :
            riid == IID_IMediaSeeking       ? "IMediaSeeking" :
            riid == IID_IAMDeviceRemoval    ? "IAMDeviceRemoval" :
            riid == IID_IAMOpenProgress     ? "IAMOpenProgress" :
            riid == IID_IMediaPosition      ? "IMediaPosition" :
            riid == IID_IMediaFilter        ? "IMediaFilter" :
            riid == IID_IBasicVideo         ? "IBasicVideo" :
            riid == IID_IBasicAudio         ? "IBasicAudio" :
            riid == IID_IVideoWindow        ? "IVideoWindow" :
            riid == IID_IUnknown            ? "IUnknown" :
            [&]() -> std::string
            {
                char buff[128];
                snprintf(buff, sizeof(buff),
                    "{%08lX-%04hX-%04hX-%02hhX%02hhX-%02hhX%02hhX%02hhX%02hhX%02hhX%02hhX}",
                    riid.Data1, riid.Data2, riid.Data3,
                    riid.Data4[0], riid.Data4[1], riid.Data4[2], riid.Data4[3],
                    riid.Data4[4], riid.Data4[5], riid.Data4[6], riid.Data4[7]);
                return buff;
            }();
    return s;
}
#define LOG(fmt, ...) [&,funcname=__func__]{ \
        if (logfile) std::fprintf(logfile, "%s: %s " fmt, NOW(), funcname, __VA_ARGS__); \
        std::printf("%s: %s " fmt, NOW(), funcname, __VA_ARGS__); \
    }()

#else // ENABLE_LOG

#define OPEN_LOGFILE()
#define CLOSE_LOGFILE()
#define LOG(...)

#endif // ENABLE_LOG


AM_MEDIA_TYPE* allocateMediaType()
{
    BYTE *pbFormat = (BYTE*)CoTaskMemAlloc(sizeof(VIDEOINFOHEADER));
    if (!pbFormat)
    {
        return nullptr;
    }

    AM_MEDIA_TYPE *amt = (AM_MEDIA_TYPE*)CoTaskMemAlloc(sizeof(AM_MEDIA_TYPE));
    if (!amt)
    {
        CoTaskMemFree(pbFormat);
        return nullptr;
    }
    std::memset(amt, 0, sizeof(*amt));
    amt->pbFormat = pbFormat;
    return amt;
}

using softcam::OutputFormat;
using softcam::OutputKind;

// Chrome's DirectShow sink tries I420, then YUY2, then UYVY, then RGB24,
// then RGB32. Discord's call capturer uses that order and also accepts
// NV12. A camera whose first type is bottom-up RGB24 connects, then the
// channel stays black. The first offered type is I420, which both accept.
const GUID SUBTYPE_I420 = FOURCCMap(MAKEFOURCC('I','4','2','0'));
const GUID SUBTYPE_NV12 = FOURCCMap(MAKEFOURCC('N','V','1','2'));

struct CatalogEntry
{
    OutputKind kind;
    int width;
    int height;
    float fps;
};

const CatalogEntry kCatalog[] =
{
    { OutputKind::I420,  1280, 720, 30 },
    { OutputKind::Nv12,  1280, 720, 30 },
    { OutputKind::Yuy2,  1280, 720, 30 },
    { OutputKind::Uyvy,  1280, 720, 30 },
    { OutputKind::Rgb32, 1280, 720, 30 },
    { OutputKind::Rgb24, 1280, 720, 30 },
    { OutputKind::I420,   640, 480, 30 },
    { OutputKind::Nv12,   640, 480, 30 },
    { OutputKind::Yuy2,   640, 480, 30 },
    { OutputKind::Uyvy,   640, 480, 30 },
    { OutputKind::Rgb32,  640, 480, 30 },
    { OutputKind::Rgb24,  640, 480, 30 },
    { OutputKind::I420,   640, 360, 30 },
    { OutputKind::Yuy2,   640, 360, 30 },
    { OutputKind::Rgb24,  640, 360, 30 },
    { OutputKind::I420,   320, 240, 30 },
    { OutputKind::Yuy2,   320, 240, 30 },
    { OutputKind::Rgb24,  320, 240, 30 },
    { OutputKind::I420,   960, 540, 30 },
    { OutputKind::Yuy2,   960, 540, 30 },
    { OutputKind::Rgb24,  960, 540, 30 },
    { OutputKind::I420,  1920,1080, 30 },
    { OutputKind::Nv12,  1920,1080, 30 },
    { OutputKind::Yuy2,  1920,1080, 30 },
    { OutputKind::Uyvy,  1920,1080, 30 },
    { OutputKind::Rgb32, 1920,1080, 30 },
    { OutputKind::Rgb24, 1920,1080, 30 },
    { OutputKind::I420,  1280, 720, 15 },
    { OutputKind::Yuy2,  1280, 720, 15 },
    { OutputKind::I420,  1280, 720, 60 },
    { OutputKind::Yuy2,  1280, 720, 60 },
    { OutputKind::I420,   640, 480, 15 },
    { OutputKind::Yuy2,   640, 480, 15 },
    { OutputKind::I420,   640, 480, 60 },
    { OutputKind::Yuy2,   640, 480, 60 },
    { OutputKind::I420,  1920,1080, 15 },
    { OutputKind::Yuy2,  1920,1080, 15 },
    { OutputKind::I420,  1920,1080, 60 },
    { OutputKind::Yuy2,  1920,1080, 60 },
};

std::size_t imageSize(OutputKind kind, int width, int height)
{
    const std::size_t pixels = static_cast<std::size_t>(width) * static_cast<std::size_t>(height);
    switch (kind)
    {
    case OutputKind::I420:
    case OutputKind::Nv12:  return pixels * 3 / 2;
    case OutputKind::Yuy2:  return pixels * 2;
    case OutputKind::Uyvy:  return pixels * 2;
    case OutputKind::Rgb32: return pixels * 4;
    case OutputKind::Rgb24:
    default:
        return pixels * 3;
    }
}

int bitsPerPixel(OutputKind kind)
{
    switch (kind)
    {
    case OutputKind::I420:
    case OutputKind::Nv12:  return 12;
    case OutputKind::Yuy2:  return 16;
    case OutputKind::Uyvy:  return 16;
    case OutputKind::Rgb32: return 32;
    case OutputKind::Rgb24:
    default:                return 24;
    }
}

DWORD compressionOf(OutputKind kind)
{
    switch (kind)
    {
    case OutputKind::I420:  return MAKEFOURCC('I','4','2','0');
    case OutputKind::Nv12:  return MAKEFOURCC('N','V','1','2');
    case OutputKind::Yuy2:  return MAKEFOURCC('Y','U','Y','2');
    case OutputKind::Uyvy:  return MAKEFOURCC('U','Y','V','Y');
    case OutputKind::Rgb32:
    case OutputKind::Rgb24:
    default:                return BI_RGB;
    }
}

GUID subtypeOf(OutputKind kind)
{
    switch (kind)
    {
    case OutputKind::I420:  return SUBTYPE_I420;
    case OutputKind::Nv12:  return SUBTYPE_NV12;
    case OutputKind::Yuy2:  return MEDIASUBTYPE_YUY2;
    case OutputKind::Uyvy:  return MEDIASUBTYPE_UYVY;
    case OutputKind::Rgb32: return MEDIASUBTYPE_RGB32;
    case OutputKind::Rgb24:
    default:                return MEDIASUBTYPE_RGB24;
    }
}

bool sameFormat(const OutputFormat& a, const OutputFormat& b)
{
    return a.kind == b.kind && a.width == b.width && a.height == b.height
        && std::fabs(a.fps - b.fps) < 0.5f;
}

std::size_t calcDIBSize(int width, int height)
{
    std::size_t stride = (static_cast<unsigned>(width) * 3 + 3) & ~3u;
    return stride * static_cast<unsigned>(height);
}

void fillMediaType(AM_MEDIA_TYPE* amt, const OutputFormat& format)
{
    BYTE *pbFormat = amt->pbFormat;
    float framerate = format.fps;
    if (framerate <= 0.0f)
    {
        framerate = 30.0f;
    }
    const int bpp = bitsPerPixel(format.kind);
    const float bit_rate = (float)format.width * (float)format.height * (float)bpp * framerate;
    const float period = 10 * 1000 * 1000 / framerate;
    const std::size_t size = imageSize(format.kind, format.width, format.height);

    VIDEOINFOHEADER* pFormat = (VIDEOINFOHEADER*)pbFormat;
    std::memset(pFormat, 0, sizeof(*pFormat));
    pFormat->dwBitRate = (uint32_t)(std::min)(bit_rate, (float)INT_MAX);
    pFormat->dwBitErrorRate = 0;
    pFormat->AvgTimePerFrame = (LONGLONG)std::round((std::min)(period, (float)LONG_MAX));
    pFormat->bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    pFormat->bmiHeader.biWidth = format.width;
    pFormat->bmiHeader.biHeight = format.height;
    pFormat->bmiHeader.biPlanes = 1;
    pFormat->bmiHeader.biBitCount = (WORD)bpp;
    pFormat->bmiHeader.biCompression = compressionOf(format.kind);
    pFormat->bmiHeader.biSizeImage = static_cast<uint32_t>(size);

    amt->majortype = MEDIATYPE_Video;
    amt->subtype = subtypeOf(format.kind);
    amt->bFixedSizeSamples = TRUE;
    amt->bTemporalCompression = FALSE;
    amt->lSampleSize = static_cast<uint32_t>(size);
    amt->formattype = FORMAT_VideoInfo;
    amt->pUnk = nullptr;
    amt->cbFormat = sizeof(VIDEOINFOHEADER);
    amt->pbFormat = pbFormat;
}

void fillMediaType(AM_MEDIA_TYPE* amt, int width, int height, float framerate)
{
    OutputFormat format;
    format.kind = OutputKind::Rgb24;
    format.width = width;
    format.height = height;
    format.fps = framerate <= 0.0f ? 60.0f : framerate;
    fillMediaType(amt, format);
}

bool kindFromSubtype(const GUID& subtype, DWORD compression, OutputKind* out)
{
    if (subtype == SUBTYPE_I420 || compression == MAKEFOURCC('I','4','2','0'))
    {
        *out = OutputKind::I420;
        return true;
    }
    if (subtype == SUBTYPE_NV12 || compression == MAKEFOURCC('N','V','1','2'))
    {
        *out = OutputKind::Nv12;
        return true;
    }
    if (subtype == MEDIASUBTYPE_YUY2 || compression == MAKEFOURCC('Y','U','Y','2'))
    {
        *out = OutputKind::Yuy2;
        return true;
    }
    if (subtype == MEDIASUBTYPE_UYVY || compression == MAKEFOURCC('U','Y','V','Y'))
    {
        *out = OutputKind::Uyvy;
        return true;
    }
    if ((subtype == MEDIASUBTYPE_RGB32 || subtype == MEDIASUBTYPE_ARGB32) && compression == BI_RGB)
    {
        *out = OutputKind::Rgb32;
        return true;
    }
    if (subtype == MEDIASUBTYPE_RGB24 && compression == BI_RGB)
    {
        *out = OutputKind::Rgb24;
        return true;
    }
    return false;
}

uint8_t clipByte(int value)
{
    if (value < 0) return 0;
    if (value > 255) return 255;
    return static_cast<uint8_t>(value);
}

void rgbToYuv(uint8_t b, uint8_t g, uint8_t r, uint8_t* y, uint8_t* u, uint8_t* v)
{
    // BT.601 limited range. Source samples are B,G,R (ffmpeg bgr24 / BI_RGB).
    const int rr = r;
    const int gg = g;
    const int bb = b;
    *y = clipByte(((66 * rr + 129 * gg + 25 * bb + 128) >> 8) + 16);
    *u = clipByte(((-38 * rr - 74 * gg + 112 * bb + 128) >> 8) + 128);
    *v = clipByte(((112 * rr - 94 * gg - 18 * bb + 128) >> 8) + 128);
}

void scaleBgr(const uint8_t* src, int sw, int sh, uint8_t* dst, int dw, int dh)
{
    if (sw <= 0 || sh <= 0 || dw <= 0 || dh <= 0)
    {
        return;
    }
    for (int y = 0; y < dh; y++)
    {
        const int sy = y * sh / dh;
        const uint8_t* src_row = src + (std::size_t)sy * sw * 3;
        uint8_t* dst_row = dst + (std::size_t)y * dw * 3;
        for (int x = 0; x < dw; x++)
        {
            const int sx = x * sw / dw;
            const uint8_t* p = src_row + sx * 3;
            dst_row[x * 3 + 0] = p[0];
            dst_row[x * 3 + 1] = p[1];
            dst_row[x * 3 + 2] = p[2];
        }
    }
}

void writeI420(const uint8_t* bgr, int width, int height, uint8_t* dst)
{
    const int chroma_w = width / 2;
    const int chroma_h = height / 2;
    uint8_t* y_plane = dst;
    uint8_t* u_plane = y_plane + width * height;
    uint8_t* v_plane = u_plane + chroma_w * chroma_h;
    for (int y = 0; y < height; y++)
    {
        const uint8_t* row = bgr + (std::size_t)y * width * 3;
        for (int x = 0; x < width; x++)
        {
            uint8_t Y, U, V;
            rgbToYuv(row[x * 3], row[x * 3 + 1], row[x * 3 + 2], &Y, &U, &V);
            y_plane[y * width + x] = Y;
            if ((y % 2) == 0 && (x % 2) == 0)
            {
                const int cx = x / 2;
                const int cy = y / 2;
                u_plane[cy * chroma_w + cx] = U;
                v_plane[cy * chroma_w + cx] = V;
            }
        }
    }
}

void writeNv12(const uint8_t* bgr, int width, int height, uint8_t* dst)
{
    // NV12 is top-down: full Y plane, then interleaved U,V pairs.
    uint8_t* y_plane = dst;
    uint8_t* uv_plane = y_plane + static_cast<std::size_t>(width) * static_cast<std::size_t>(height);
    for (int y = 0; y < height; y++)
    {
        const uint8_t* row = bgr + (std::size_t)y * width * 3;
        for (int x = 0; x < width; x++)
        {
            uint8_t Y, U, V;
            rgbToYuv(row[x * 3], row[x * 3 + 1], row[x * 3 + 2], &Y, &U, &V);
            y_plane[y * width + x] = Y;
            if ((y % 2) == 0 && (x % 2) == 0)
            {
                uint8_t* uv = uv_plane + (std::size_t)(y / 2) * width + x;
                uv[0] = U;
                uv[1] = V;
            }
        }
    }
}

void writeYuy2(const uint8_t* bgr, int width, int height, uint8_t* dst, bool uyvy)
{
    for (int y = 0; y < height; y++)
    {
        const uint8_t* row = bgr + (std::size_t)y * width * 3;
        uint8_t* out = dst + (std::size_t)y * width * 2;
        for (int x = 0; x < width; x += 2)
        {
            uint8_t y0, u0, v0, y1, u1, v1;
            rgbToYuv(row[x * 3], row[x * 3 + 1], row[x * 3 + 2], &y0, &u0, &v0);
            const int x1 = (std::min)(x + 1, width - 1);
            rgbToYuv(row[x1 * 3], row[x1 * 3 + 1], row[x1 * 3 + 2], &y1, &u1, &v1);
            const uint8_t u = static_cast<uint8_t>((u0 + u1) / 2);
            const uint8_t v = static_cast<uint8_t>((v0 + v1) / 2);
            uint8_t* p = out + x * 2;
            if (uyvy)
            {
                p[0] = u; p[1] = y0; p[2] = v; p[3] = y1;
            }
            else
            {
                p[0] = y0; p[1] = u; p[2] = y1; p[3] = v;
            }
        }
    }
}

void writeRgb(const uint8_t* bgr, int width, int height, uint8_t* dst, bool rgba)
{
    // BI_RGB is a bottom-up DIB. The first row in the buffer is the bottom
    // of the picture. I420 and YUY2 stay top-down.
    const int channels = rgba ? 4 : 3;
    for (int y = 0; y < height; y++)
    {
        const uint8_t* src = bgr + (std::size_t)(height - 1 - y) * width * 3;
        uint8_t* out = dst + (std::size_t)y * width * channels;
        for (int x = 0; x < width; x++)
        {
            out[x * channels + 0] = src[x * 3 + 0];
            out[x * channels + 1] = src[x * 3 + 1];
            out[x * channels + 2] = src[x * 3 + 2];
            if (rgba)
            {
                out[x * channels + 3] = 0xff;
            }
        }
    }
}

AM_MEDIA_TYPE* makeMediaType(const OutputFormat& format)
{
    AM_MEDIA_TYPE *amt = allocateMediaType();
    if (!amt)
    {
        return nullptr;
    }
    fillMediaType(amt, format);
    return amt;
}

AM_MEDIA_TYPE* makeMediaType(int width, int height, float framerate)
{
    OutputFormat format;
    format.kind = OutputKind::Rgb24;
    format.width = width;
    format.height = height;
    format.fps = framerate;
    return makeMediaType(format);
}

} //namespace

namespace softcam {


CUnknown * Softcam::CreateInstance(
                    LPUNKNOWN   lpunk,
                    const GUID& clsid,
                    HRESULT*    phr)
{
    OPEN_LOGFILE();
    LOG("===== logging started =====\n");

    return new Softcam(lpunk, clsid, phr);
}

Softcam::Softcam(LPUNKNOWN lpunk, const GUID& clsid, HRESULT *phr) :
    CSource(NAME("ReCast Camera"), lpunk, clsid),
    m_frame_buffer(FrameBuffer::open()),
    m_valid(m_frame_buffer ? true : false),
    m_width(m_frame_buffer.width()),
    m_height(m_frame_buffer.height()),
    m_framerate(m_frame_buffer.framerate())
{
    // This code is okay though it may look strange as the return value is ignored.
    // Calling the SoftcamStream constructor results in calling the CBaseOutputPin
    // constructor which registers the instance to this Softcam instance by calling
    // CSource::AddPin().
    (void)new SoftcamStream(phr, this, L"ReCast Camera Stream");
}


STDMETHODIMP Softcam::NonDelegatingQueryInterface(REFIID riid, __deref_out void **ppv)
{
    if (riid == IID_IAMStreamConfig)
    {
        LOG("(Softcam) IAMStreamConfig -> S_OK\n");
        return GetInterface(static_cast<IAMStreamConfig*>(this), ppv);
    }
    else if (riid == IID_IAMFilterMiscFlags)
    {
        LOG("(Softcam) IAMFilterMiscFlags -> S_OK\n");
        return GetInterface(static_cast<IAMFilterMiscFlags*>(this), ppv);
    }
    else
    {
        auto result = CSource::NonDelegatingQueryInterface(riid, ppv);
        LOG("(Softcam) %s -> %s\n", IID_TO_STR(riid).c_str(), result ? "(ERROR)" : "S_OK");
        return result;
    }
}

OutputFormat Softcam::nativeFormat() const
{
    OutputFormat format;
    // Discord connects the first pin type when it does not call SetFormat.
    // I420 is top-down and is the first type both Discord and Chrome accept.
    format.kind = OutputKind::I420;
    if (m_frame_buffer)
    {
        format.width = m_frame_buffer.width();
        format.height = m_frame_buffer.height();
        format.fps = m_frame_buffer.framerate();
    }
    else
    {
        format.width = m_width > 0 ? m_width : 1280;
        format.height = m_height > 0 ? m_height : 720;
        format.fps = m_framerate > 0.0f ? m_framerate : 30.0f;
    }
    if (format.width <= 0 || format.height <= 0)
    {
        format.width = 1280;
        format.height = 720;
    }
    if (format.fps <= 0.0f)
    {
        format.fps = 30.0f;
    }
    return format;
}

int Softcam::width() const
{
    CAutoLock lock(&m_critsec);
    if (m_frame_buffer)
    {
        return m_frame_buffer.width();
    }
    return m_width;
}

int Softcam::height() const
{
    CAutoLock lock(&m_critsec);
    if (m_frame_buffer)
    {
        return m_frame_buffer.height();
    }
    return m_height;
}

float Softcam::framerate() const
{
    CAutoLock lock(&m_critsec);
    if (m_frame_buffer)
    {
        return m_frame_buffer.framerate();
    }
    return m_framerate;
}

OutputFormat Softcam::formatAt(int index) const
{
    CAutoLock lock(&m_critsec);
    OutputFormat preferred = m_has_override ? m_override : nativeFormat();
    if (index <= 0)
    {
        return preferred;
    }
    int seen = 0;
    for (const CatalogEntry& entry : kCatalog)
    {
        OutputFormat format;
        format.kind = entry.kind;
        format.width = entry.width;
        format.height = entry.height;
        format.fps = entry.fps;
        if (sameFormat(format, preferred))
        {
            continue;
        }
        seen += 1;
        if (seen == index)
        {
            return format;
        }
    }
    return preferred;
}

int Softcam::formatCount() const
{
    CAutoLock lock(&m_critsec);
    OutputFormat preferred = m_has_override ? m_override : nativeFormat();
    int count = 1;
    for (const CatalogEntry& entry : kCatalog)
    {
        OutputFormat format;
        format.kind = entry.kind;
        format.width = entry.width;
        format.height = entry.height;
        format.fps = entry.fps;
        if (!sameFormat(format, preferred))
        {
            count += 1;
        }
    }
    return count;
}

bool Softcam::acceptMediaType(const AM_MEDIA_TYPE* mt, OutputFormat* out) const
{
    if (!mt || mt->majortype != MEDIATYPE_Video || mt->formattype != FORMAT_VideoInfo || !mt->pbFormat)
    {
        return false;
    }
    if (mt->cbFormat < sizeof(VIDEOINFOHEADER))
    {
        return false;
    }
    const VIDEOINFOHEADER* header = reinterpret_cast<const VIDEOINFOHEADER*>(mt->pbFormat);
    const int width = header->bmiHeader.biWidth;
    const LONG raw_height = header->bmiHeader.biHeight;
    const int height = raw_height < 0 ? static_cast<int>(-raw_height) : static_cast<int>(raw_height);
    // Width multiple of 4 keeps RGB strides tight and YUV chroma even.
    if (width < 4 || height < 2 || width > 4096 || height > 4096 ||
        (width % 4) != 0 || (height % 2) != 0)
    {
        return false;
    }
    OutputKind kind;
    if (!kindFromSubtype(mt->subtype, header->bmiHeader.biCompression, &kind))
    {
        return false;
    }
    if (kind == OutputKind::Rgb24 && header->bmiHeader.biBitCount != 24)
    {
        return false;
    }
    if (kind == OutputKind::Rgb32 && header->bmiHeader.biBitCount != 32)
    {
        return false;
    }
    float fps = 30.0f;
    if (header->AvgTimePerFrame > 0)
    {
        fps = 10000000.0f / static_cast<float>(header->AvgTimePerFrame);
    }
    if (fps < 1.0f || fps > 240.0f)
    {
        fps = 30.0f;
    }
    if (out)
    {
        out->kind = kind;
        out->width = width;
        out->height = height;
        out->fps = fps;
    }
    return true;
}

HRESULT
Softcam::SetFormat(AM_MEDIA_TYPE *mt)
{
    if (!mt)
    {
        LOG("-> E_POINTER\n");
        return E_POINTER;
    }
    OutputFormat format;
    if (!acceptMediaType(mt, &format))
    {
        LOG("-> E_FAIL (invalid media type)\n");
        return E_FAIL;
    }
    {
        CAutoLock lock(&m_critsec);
        m_override = format;
        m_has_override = true;
    }
    LOG("-> S_OK\n");
    return S_OK;
}

HRESULT
Softcam::GetFormat(AM_MEDIA_TYPE **out_pmt)
{
    if (!out_pmt)
    {
        LOG("-> E_POINTER\n");
        return E_POINTER;
    }
    AM_MEDIA_TYPE* mt = makeMediaType(formatAt(0));
    if (!mt)
    {
        LOG("-> E_OUTOFMEMORY\n");
        return E_OUTOFMEMORY;
    }
    *out_pmt = mt;
    LOG("-> S_OK\n");
    return S_OK;
}

HRESULT
Softcam::GetNumberOfCapabilities(int *out_count, int *out_size)
{
    if (!out_count || !out_size)
    {
        LOG("-> E_POINTER\n");
        return E_POINTER;
    }
    *out_count = formatCount();
    *out_size = sizeof(VIDEO_STREAM_CONFIG_CAPS);
    LOG("-> S_OK\n");
    return S_OK;
}

HRESULT
Softcam::GetStreamCaps(int index, AM_MEDIA_TYPE **out_pmt, BYTE *out_scc)
{
    if (!out_pmt || !out_scc)
    {
        LOG("-> E_POINTER\n");
        return E_POINTER;
    }
    if (index < 0 || index >= formatCount())
    {
        LOG("-> S_FALSE (invalid index)\n");
        return S_FALSE;
    }
    OutputFormat chosen = formatAt(index);
    AM_MEDIA_TYPE *mt = makeMediaType(chosen);
    if (!mt)
    {
        LOG("-> E_OUTOFMEMORY\n");
        return E_OUTOFMEMORY;
    }
    *out_pmt = mt;
    std::memset(out_scc, 0, sizeof(VIDEO_STREAM_CONFIG_CAPS));
    VIDEOINFOHEADER* format = (VIDEOINFOHEADER*)mt->pbFormat;
    VIDEO_STREAM_CONFIG_CAPS* scc = (VIDEO_STREAM_CONFIG_CAPS*)out_scc;
    scc->guid = FORMAT_VideoInfo;
    scc->InputSize = SIZE{chosen.width, chosen.height};
    scc->MinCroppingSize = SIZE{chosen.width, chosen.height};
    scc->MaxCroppingSize = SIZE{chosen.width, chosen.height};
    scc->CropGranularityX = 1;
    scc->CropGranularityY = 1;
    scc->CropAlignX = 1;
    scc->CropAlignY = 1;
    scc->MinOutputSize = SIZE{chosen.width, chosen.height};
    scc->MaxOutputSize = SIZE{chosen.width, chosen.height};
    scc->OutputGranularityX = 1;
    scc->OutputGranularityY = 1;
    scc->StretchTapsX = 0;
    scc->StretchTapsY = 0;
    scc->ShrinkTapsX = 0;
    scc->ShrinkTapsY = 0;
    scc->MinFrameInterval = format->AvgTimePerFrame;
    scc->MaxFrameInterval = format->AvgTimePerFrame;
    scc->MinBitsPerSecond = (LONG)format->dwBitRate;
    scc->MaxBitsPerSecond = (LONG)format->dwBitRate;
    LOG("-> S_OK\n");
    return S_OK;
}


FrameBuffer* Softcam::getFrameBuffer()
{
    if (!m_valid)
    {
        return nullptr;
    }

    CAutoLock lock(&m_critsec);
    if (!m_frame_buffer)
    {
        auto fb = FrameBuffer::open();
        // Any active sender is usable. The pin scales into the format the
        // application connected with, so a resolution change is not a new device.
        if (fb && fb.active())
        {
            m_frame_buffer = fb;
        }
    }
    if (m_frame_buffer)
    {
        return &m_frame_buffer;
    }
    else
    {
        return nullptr;
    }
}

void
Softcam::releaseFrameBuffer()
{
    CAutoLock lock(&m_critsec);
    m_frame_buffer.release();
}

SoftcamStream::SoftcamStream(HRESULT *phr,
                         Softcam *pParent,
                         LPCWSTR pPinName) :
    CSourceStream(NAME("ReCast Camera Stream"), phr, pParent, pPinName),
    m_valid(pParent->valid()),
    m_width(pParent->width()),
    m_height(pParent->height())
{
}


SoftcamStream::~SoftcamStream()
{
    LOG("===== logging finished =====\n");
    CLOSE_LOGFILE();
}


STDMETHODIMP SoftcamStream::NonDelegatingQueryInterface(REFIID riid, __deref_out void **ppv)
{
    if (riid == IID_IKsPropertySet )
    {
        LOG("(SoftcamStream) IKsPropertySet -> S_OK\n");
        return GetInterface(static_cast<IKsPropertySet*>(this), ppv);
    }
    else if(riid == IID_IAMStreamConfig)
    {
        LOG("(SoftcamStream) IAMStreamConfig -> S_OK\n");
        return GetInterface(static_cast<IAMStreamConfig*>(this), ppv);
    }
    else
    {
        auto result = CSourceStream::NonDelegatingQueryInterface(riid, ppv);
        LOG("(SoftcamStream) %s -> %s\n", IID_TO_STR(riid).c_str(), result ? "(ERROR)" : "S_OK");
        return result;
    }
}


HRESULT SoftcamStream::CheckMediaType(const CMediaType *pMediaType)
{
    CheckPointer(pMediaType, E_POINTER);
    OutputFormat format;
    if (!getParent()->acceptMediaType(pMediaType, &format))
    {
        return E_FAIL;
    }
    return S_OK;
}

HRESULT SoftcamStream::GetMediaType(int iPosition, CMediaType *pMediaType)
{
    CheckPointer(pMediaType, E_POINTER);
    if (iPosition < 0)
    {
        return E_INVALIDARG;
    }
    if (iPosition >= getParent()->formatCount())
    {
        return VFW_S_NO_MORE_ITEMS;
    }
    OutputFormat format = getParent()->formatAt(iPosition);
    VIDEOINFOHEADER *pvi = (VIDEOINFOHEADER*)pMediaType->AllocFormatBuffer(sizeof(VIDEOINFOHEADER));
    if (pvi == nullptr)
    {
        return E_OUTOFMEMORY;
    }
    fillMediaType(pMediaType, format);
    return S_OK;
}

HRESULT SoftcamStream::SetMediaType(const CMediaType *pMediaType)
{
    HRESULT hr = CSourceStream::SetMediaType(pMediaType);
    if (FAILED(hr))
    {
        return hr;
    }
    OutputFormat format;
    if (pMediaType && getParent()->acceptMediaType(pMediaType, &format))
    {
        m_has_connected = true;
        m_connected = format;
    }
    return hr;
}

HRESULT SoftcamStream::DoBufferProcessingLoop(void)
{
    Command com;
    OnThreadStartPlay();

    do
    {
        while (!CheckRequest(&com))
        {
            // Wait for the next picture before taking a sample. Holding the
            // only sample while waiting makes the next open fail with
            // "already in use" because the previous graph has not stopped.
            if (auto fb = getParent()->getFrameBuffer())
            {
                fb->waitForNewFrame(m_frame_counter, 0.02f);
            }
            else
            {
                Sleep(20);
            }
            if (CheckRequest(&com))
            {
                break;
            }

            IMediaSample *pSample = nullptr;
            HRESULT hr = GetDeliveryBuffer(&pSample, nullptr, nullptr, 0);
            if (FAILED(hr))
            {
                Sleep(1);
                continue;
            }

            hr = fillBufferNow(pSample);
            if (hr == S_OK)
            {
                hr = Deliver(pSample);
                pSample->Release();
                if (hr != S_OK)
                {
                    return S_OK;
                }
            }
            else if (hr == S_FALSE)
            {
                pSample->Release();
                DeliverEndOfStream();
                return S_OK;
            }
            else
            {
                pSample->Release();
                DeliverEndOfStream();
                m_pFilter->NotifyEvent(EC_ERRORABORT, hr, 0);
                return hr;
            }
        }

        if (com == CMD_RUN || com == CMD_PAUSE)
        {
            Reply(NOERROR);
        }
        else if (com != CMD_STOP)
        {
            Reply((DWORD)E_UNEXPECTED);
        }
    }
    while (com != CMD_STOP);

    return S_FALSE;
}

HRESULT SoftcamStream::FillBuffer(IMediaSample *pms)
{
    CheckPointer(pms, E_POINTER);
    if (auto fb = getParent()->getFrameBuffer())
    {
        fb->waitForNewFrame(m_frame_counter, 0.5f);
    }
    return fillBufferNow(pms);
}

HRESULT SoftcamStream::renderConnected(IMediaSample *pms)
{
    BYTE *pData = nullptr;
    pms->GetPointer(&pData);
    const long capacity = pms->GetSize();
    const OutputFormat format = m_connected;
    const std::size_t need = imageSize(format.kind, format.width, format.height);
    if (!pData || capacity < (long)need)
    {
        return E_FAIL;
    }
    ZeroMemory(pData, need);

    int src_w = 0;
    int src_h = 0;
    bool have_source = false;
    if (auto fb = getParent()->getFrameBuffer())
    {
        int probe_w = 0;
        int probe_h = 0;
        uint64_t counter = m_frame_counter;
        fb->copyImage(nullptr, 0, &probe_w, &probe_h, &counter);
        if (probe_w > 0 && probe_h > 0)
        {
            const std::size_t bytes = (std::size_t)probe_w * (std::size_t)probe_h * 3;
            if (m_source.size() < bytes)
            {
                m_source.resize(bytes);
            }
            have_source = fb->copyImage(m_source.data(), m_source.size(), &src_w, &src_h, &m_frame_counter);
        }
    }

    const std::size_t scaled_bytes = (std::size_t)format.width * (std::size_t)format.height * 3;
    if (m_scaled.size() < scaled_bytes)
    {
        m_scaled.resize(scaled_bytes);
    }
    if (have_source && src_w > 0 && src_h > 0)
    {
        scaleBgr(m_source.data(), src_w, src_h, m_scaled.data(), format.width, format.height);
    }
    else
    {
        std::memset(m_scaled.data(), 0, scaled_bytes);
    }

    switch (format.kind)
    {
    case OutputKind::I420:
        writeI420(m_scaled.data(), format.width, format.height, pData);
        break;
    case OutputKind::Nv12:
        writeNv12(m_scaled.data(), format.width, format.height, pData);
        break;
    case OutputKind::Yuy2:
        writeYuy2(m_scaled.data(), format.width, format.height, pData, false);
        break;
    case OutputKind::Uyvy:
        writeYuy2(m_scaled.data(), format.width, format.height, pData, true);
        break;
    case OutputKind::Rgb32:
        writeRgb(m_scaled.data(), format.width, format.height, pData, true);
        break;
    case OutputKind::Rgb24:
    default:
        writeRgb(m_scaled.data(), format.width, format.height, pData, false);
        break;
    }

    pms->SetActualDataLength(static_cast<long>(need));
    CAutoLock lock(&m_critsec);
    CRefTime start = m_sample_time;
    m_sample_time += (LONG)m_interval_time_msec;
    pms->SetTime((REFERENCE_TIME*)&start, (REFERENCE_TIME*)&m_sample_time);
    pms->SetSyncPoint(TRUE);
    return NOERROR;
}

HRESULT SoftcamStream::fillBufferNow(IMediaSample *pms)
{
    // A connected application gets the format it negotiated. The direct
    // FillBuffer tests have no connection and still expect the original
    // bottom-up RGB picture at the sender's size.
    if (m_has_connected)
    {
        return renderConnected(pms);
    }

    BYTE *pData;
    pms->GetPointer(&pData);
    long lDataLen = pms->GetSize();
    if (!pData || lDataLen < 0)
    {
        return E_FAIL;
    }
    ZeroMemory(pData, (std::size_t)lDataLen);
    {
        if (auto fb = getParent()->getFrameBuffer())
        {
            // transferToDIB writes the whole sender frame. A sample that is
            // smaller than that frame must not be written past its end.
            const std::size_t size = calcDIBSize(fb->width(), fb->height());
            if (size > (std::size_t)lDataLen)
            {
                return E_FAIL;
            }
            bool active = fb->active();
            fb->transferToDIB(pData, &m_frame_counter);

            if (!active)
            {
                // The sender has deactivated this stream and stopped sending frames.
                // We release this stream and will wait a new stream to be available.
                getParent()->releaseFrameBuffer();

                // Save the last image for a placeholder. `size` is the frame
                // that transferToDIB just wrote, already checked against the sample.
                if (!m_screenshot)
                {
                    m_screenshot.reset(new uint8_t[size]);
                }
                {
                    // Darken the image to indicate that the source is inactive.
                    for (std::size_t i = 0; i < size; i++)
                    {
                        pData[i] /= 4;
                    }
                }
                std::memcpy(m_screenshot.get(), pData, size);
            }
        }
        else
        {
            // Waiting for a new stream.
            m_frame_counter = 0;
            Timer::sleep(0.100f);

            const std::size_t size = calcDIBSize(m_width, m_height);
            if (size > (std::size_t)lDataLen)
            {
                return E_FAIL;
            }
            if (m_screenshot)
            {
                std::memcpy(pData, m_screenshot.get(), size);
            }
            else
            {
                // Nothing was received yet, so the frame stays black.
                ZeroMemory(pData, size);
            }
        }

        CAutoLock lock(&m_critsec);
        CRefTime start = m_sample_time;
        m_sample_time += (LONG)m_interval_time_msec;
        pms->SetTime((REFERENCE_TIME*)&start,(REFERENCE_TIME*)&m_sample_time);
    }
    pms->SetSyncPoint(TRUE);
    //LOG("-> NOERROR\n");
    return NOERROR;
}


STDMETHODIMP SoftcamStream::Notify(IBaseFilter * pSender, Quality q)
{
    CAutoLock lock(&m_critsec);
    m_interval_time_msec = m_interval_time_msec * 1000 / (std::max)(q.Proportion, 1L);
    m_interval_time_msec = (std::min)((std::max)(m_interval_time_msec, 1L), 1000L);
    if (q.Late > 0) {
        m_sample_time += q.Late;
    }
    //LOG("-> NOERROR\n");
    return NOERROR;
}


HRESULT SoftcamStream::GetMediaType(CMediaType *pmt)
{
    CheckPointer(pmt,E_POINTER);

    VIDEOINFOHEADER *pvi = (VIDEOINFOHEADER*)pmt->AllocFormatBuffer(sizeof(VIDEOINFOHEADER));
    if (pvi == nullptr)
    {
        LOG("-> E_OUTOFMEMORY\n");
        return E_OUTOFMEMORY;
    }

    fillMediaType(pmt, getParent()->formatAt(0));

    LOG("-> NOERROR\n");
    return NOERROR;
}

HRESULT SoftcamStream::DecideBufferSize(IMemAllocator *pAlloc,
                                        ALLOCATOR_PROPERTIES *pProperties)
{
    CheckPointer(pAlloc,E_POINTER);
    CheckPointer(pProperties,E_POINTER);

    CAutoLock lock(m_pFilter->pStateLock());
    HRESULT hr = NOERROR;

    VIDEOINFO *pvi = (VIDEOINFO *)m_mt.Format();
    // More than one buffer so a consumer can hold a frame while the next
    // one is filled. A single buffer makes Stop() stall and the next open
    // reports the camera as already in use.
    pProperties->cBuffers = 4;
    pProperties->cbBuffer = (long)pvi->bmiHeader.biSizeImage;
    if (m_has_connected)
    {
        const long need = (long)imageSize(m_connected.kind, m_connected.width, m_connected.height);
        if (pProperties->cbBuffer < need)
        {
            pProperties->cbBuffer = need;
        }
    }

    ALLOCATOR_PROPERTIES actual;
    hr = pAlloc->SetProperties(pProperties, &actual);
    if (FAILED(hr))
    {
        LOG("-> (FAILED)\n");
        return hr;
    }
    if (actual.cbBuffer < pProperties->cbBuffer)
    {
        LOG("-> E_FAIL\n");
        return E_FAIL;
    }
    LOG("-> NOERROR\n");
    return NOERROR;
}

HRESULT SoftcamStream::OnThreadCreate()
{
    CAutoLock lock(&m_critsec);
    m_sample_time = 0;
    float framerate = getParent()->framerate();
    if (framerate <= 0.0f)
    {
        framerate = 60.0f;
    }
    framerate = (std::min)((std::max)(framerate, 1.0f), 1000.0f);
    m_interval_time_msec = (long)std::round(1000.0f / framerate);

    LOG("-> NOERROR\n");
    return NOERROR;
}

HRESULT SoftcamStream::Set(REFGUID guidPropSet, DWORD dwPropID,
                           LPVOID pInstanceData, DWORD cbInstanceData,
                           LPVOID pPropData, DWORD cbPropData)
{
    LOG("-> E_NOTIMPL\n");
    return E_NOTIMPL;
}

HRESULT SoftcamStream::Get(REFGUID guidPropSet, DWORD dwPropID,
                           LPVOID pInstanceData, DWORD cbInstanceData,
                           LPVOID pPropData, DWORD cbPropData, DWORD *pcbReturned)
{
    if (guidPropSet != AMPROPSETID_Pin)
    {
        LOG("-> E_PROP_SET_UNSUPPORTED\n");
        return E_PROP_SET_UNSUPPORTED;
    }
    if (dwPropID != AMPROPERTY_PIN_CATEGORY)
    {
        LOG("-> E_PROP_ID_UNSUPPORTED\n");
        return E_PROP_ID_UNSUPPORTED;
    }
    if (pPropData == nullptr && pcbReturned == nullptr)
    {
        LOG("-> E_POINTER\n");
        return E_POINTER;
    }
    if (pcbReturned)
    {
        *pcbReturned = sizeof(GUID);
    }
    if (pPropData)
    {
        if (cbPropData < sizeof(GUID))
        {
            LOG("-> E_UNEXPECTED\n");
            return E_UNEXPECTED;
        }
        *(GUID*)pPropData = PIN_CATEGORY_CAPTURE;
    }
    LOG("-> S_OK\n");
    return S_OK;
}

HRESULT SoftcamStream::QuerySupported(REFGUID guidPropSet, DWORD dwPropID,
                              DWORD *pTypeSupport)
{
    if (guidPropSet != AMPROPSETID_Pin)
    {
        LOG("-> E_PROP_SET_UNSUPPORTED\n");
        return E_PROP_SET_UNSUPPORTED;
    }
    if (dwPropID != AMPROPERTY_PIN_CATEGORY)
    {
        LOG("-> E_PROP_ID_UNSUPPORTED\n");
        return E_PROP_ID_UNSUPPORTED;
    }
    if (pTypeSupport)
    {
        *pTypeSupport = KSPROPERTY_SUPPORT_GET;
    }
    LOG("-> S_OK\n");
    return S_OK;
}

HRESULT SoftcamStream::SetFormat(AM_MEDIA_TYPE *mt)
{
    return getParent()->SetFormat(mt);
}

HRESULT SoftcamStream::GetFormat(AM_MEDIA_TYPE **out_pmt)
{
    return getParent()->GetFormat(out_pmt);
}

HRESULT SoftcamStream::GetNumberOfCapabilities(int *out_count, int *out_size)
{
    return getParent()->GetNumberOfCapabilities(out_count, out_size);
}

HRESULT SoftcamStream::GetStreamCaps(int index, AM_MEDIA_TYPE **out_pmt, BYTE *out_scc)
{
    return getParent()->GetStreamCaps(index, out_pmt, out_scc);
}

Softcam* SoftcamStream::getParent()
{
    return static_cast<Softcam*>(m_pFilter);
}


} //namespace softcam
