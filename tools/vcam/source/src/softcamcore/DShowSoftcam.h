#pragma once

#include <memory>
#include <vector>
#include <baseclasses/streams.h>
#include "FrameBuffer.h"


namespace softcam {

/// Pixel layouts browsers and meeting apps actually connect.
/// Rgb24 is the original softcam layout (bottom-up BGR).
enum class OutputKind
{
    Rgb24 = 0,
    I420,
    Yuy2,
    Uyvy,
    Rgb32,
};

struct OutputFormat
{
    OutputKind kind = OutputKind::Rgb24;
    int width = 1280;
    int height = 720;
    float fps = 30.0f;
};

class Softcam : public CSource, public IAMStreamConfig, public IAMFilterMiscFlags
{
public:
    static CUnknown* CreateInstance(
                    LPUNKNOWN   lpunk,
                    const GUID& clsid,
                    HRESULT*    phr);

    // IUnknown Methods
    DECLARE_IUNKNOWN
    STDMETHODIMP NonDelegatingQueryInterface(REFIID riid, __deref_out void **ppv) override;

    // IAMStreamConfig
    HRESULT STDMETHODCALLTYPE SetFormat(AM_MEDIA_TYPE *mt) override;
    HRESULT STDMETHODCALLTYPE GetFormat(AM_MEDIA_TYPE **out_pmt) override;
    HRESULT STDMETHODCALLTYPE GetNumberOfCapabilities(int *out_count, int *out_size) override;
    HRESULT STDMETHODCALLTYPE GetStreamCaps(int index, AM_MEDIA_TYPE **out_pmt, BYTE *out_scc) override;

    // IAMFilterMiscFlags — capture graph builders use this to recognise a source.
    STDMETHODIMP_(ULONG) GetMiscFlags() override { return AM_FILTER_MISC_FLAGS_IS_SOURCE; }

    FrameBuffer*    getFrameBuffer();
    bool            valid() const { return m_valid; }
    int             width() const;
    int             height() const;
    float           framerate() const;
    void            releaseFrameBuffer();

    /// Formats offered to browsers. Index 0 is the format a new connection
    /// should use (the sender's own RGB24 size, or the last SetFormat call).
    int             formatCount() const;
    OutputFormat    formatAt(int index) const;
    bool            acceptMediaType(const AM_MEDIA_TYPE* mt, OutputFormat* out) const;

private:
    mutable CCritSec m_critsec;
    FrameBuffer m_frame_buffer;
    const bool  m_valid;
    const int   m_width;
    const int   m_height;
    const float m_framerate;
    bool        m_has_override = false;
    OutputFormat m_override{};

    Softcam(LPUNKNOWN lpunk, const GUID& clsid, HRESULT *phr);

    OutputFormat nativeFormat() const;
};


class SoftcamStream : public CSourceStream, public IKsPropertySet, public IAMStreamConfig
{
 public:
    SoftcamStream(HRESULT *phr, Softcam *pParent, LPCWSTR pPinName);
    ~SoftcamStream();

    // IUnknown
    DECLARE_IUNKNOWN
    STDMETHODIMP NonDelegatingQueryInterface(REFIID riid, __deref_out void **ppv) override;

    // CBasePin
    STDMETHODIMP Notify(IBaseFilter * pSender, Quality q) override;
    HRESULT CheckMediaType(const CMediaType *pMediaType) override;
    HRESULT GetMediaType(int iPosition, __inout CMediaType *pMediaType) override;
    HRESULT SetMediaType(const CMediaType *pMediaType) override;

    // CBaseOutputPin
    // Ask for buffers of the size appropriate to the agreed media type
    HRESULT DecideBufferSize(IMemAllocator *pIMemAlloc,
                             ALLOCATOR_PROPERTIES *pProperties) override;

    // CSourceStream
    HRESULT FillBuffer(IMediaSample *pms) override;
    HRESULT GetMediaType(CMediaType *pMediaType) override;
    HRESULT OnThreadCreate(void) override;
    HRESULT DoBufferProcessingLoop(void) override;

    //  IKsPropertySet
    HRESULT STDMETHODCALLTYPE Set(REFGUID guidPropSet, DWORD dwPropID,
                                  LPVOID pInstanceData, DWORD cbInstanceData,
                                  LPVOID pPropData, DWORD cbPropData) override;
    HRESULT STDMETHODCALLTYPE Get(REFGUID guidPropSet, DWORD dwPropID,
                                  LPVOID pInstanceData, DWORD cbInstanceData,
                                  LPVOID pPropData, DWORD cbPropData, DWORD *pcbReturned) override;
    HRESULT STDMETHODCALLTYPE QuerySupported(REFGUID guidPropSet, DWORD dwPropID,
                                             DWORD *pTypeSupport) override;

    // IAMStreamConfig
    HRESULT STDMETHODCALLTYPE SetFormat(AM_MEDIA_TYPE *mt) override;
    HRESULT STDMETHODCALLTYPE GetFormat(AM_MEDIA_TYPE **out_pmt) override;
    HRESULT STDMETHODCALLTYPE GetNumberOfCapabilities(int *out_count, int *out_size) override;
    HRESULT STDMETHODCALLTYPE GetStreamCaps(int index, AM_MEDIA_TYPE **out_pmt, BYTE *out_scc) override;

private:
    const bool  m_valid;
    const int   m_width;
    const int   m_height;
    uint64_t    m_frame_counter = 0;
    std::unique_ptr<uint8_t[]>  m_screenshot;

    CCritSec m_critsec;
    CRefTime m_sample_time;
    long m_interval_time_msec = 10;
    OutputFormat m_connected{};
    bool m_has_connected = false;
    std::vector<uint8_t> m_source;
    std::vector<uint8_t> m_scaled;

    Softcam*        getParent();
    HRESULT         fillBufferNow(IMediaSample *pms);
    HRESULT         renderConnected(IMediaSample *pms);
};


} //namespace softcam
