// Generated by `wit-bindgen-wrpc-go` 0.1.0. DO NOT EDIT!
package streams

import (
	context "context"
	binary "encoding/binary"
	errors "errors"
	fmt "fmt"
	wasi__io__error "github.com/wrpc/wrpc/examples/go/http-outgoing-client/bindings/wasi/io/error"
	wasi__io__poll "github.com/wrpc/wrpc/examples/go/http-outgoing-client/bindings/wasi/io/poll"
	wrpc "github.com/wrpc/wrpc/go"
	io "io"
	slog "log/slog"
)

type Error = wasi__io__error.Error
type Pollable = wasi__io__poll.Pollable

// An error for input-stream and output-stream operations.
type StreamError struct {
	payload      any
	discriminant StreamErrorDiscriminant
}

func (v *StreamError) Discriminant() StreamErrorDiscriminant { return v.discriminant }

type StreamErrorDiscriminant uint8

const (
	// The last operation (a write or flush) failed before completion.
	//
	// More information is available in the `error` payload.
	StreamErrorDiscriminant_LastOperationFailed StreamErrorDiscriminant = 0
	// The stream is closed: no more input will be accepted by the
	// stream. A closed output-stream will return this error on all
	// future operations.
	StreamErrorDiscriminant_Closed StreamErrorDiscriminant = 1
)

func (v *StreamError) String() string {
	switch v.discriminant {
	case StreamErrorDiscriminant_LastOperationFailed:
		return "last-operation-failed"
	case StreamErrorDiscriminant_Closed:
		return "closed"
	default:
		panic("invalid variant")
	}
}

// The last operation (a write or flush) failed before completion.
//
// More information is available in the `error` payload.
func (v *StreamError) GetLastOperationFailed() (payload Error, ok bool) {
	if ok = (v.discriminant == StreamErrorDiscriminant_LastOperationFailed); !ok {
		return
	}
	payload, ok = v.payload.(Error)
	return
}

// The last operation (a write or flush) failed before completion.
//
// More information is available in the `error` payload.
func (v *StreamError) SetLastOperationFailed(payload Error) *StreamError {
	v.discriminant = StreamErrorDiscriminant_LastOperationFailed
	v.payload = payload
	return v
}

// The last operation (a write or flush) failed before completion.
//
// More information is available in the `error` payload.
func (StreamError) NewLastOperationFailed(payload Error) *StreamError {
	return (&StreamError{}).SetLastOperationFailed(
		payload)
}

// The stream is closed: no more input will be accepted by the
// stream. A closed output-stream will return this error on all
// future operations.
func (v *StreamError) GetClosed() (ok bool) {
	if ok = (v.discriminant == StreamErrorDiscriminant_Closed); !ok {
		return
	}
	return
}

// The stream is closed: no more input will be accepted by the
// stream. A closed output-stream will return this error on all
// future operations.
func (v *StreamError) SetClosed() *StreamError {
	v.discriminant = StreamErrorDiscriminant_Closed
	v.payload = nil
	return v
}

// The stream is closed: no more input will be accepted by the
// stream. A closed output-stream will return this error on all
// future operations.
func (StreamError) NewClosed() *StreamError {
	return (&StreamError{}).SetClosed()
}
func (v *StreamError) Error() string { return v.String() }
func (v *StreamError) WriteToIndex(w wrpc.ByteWriter) (func(wrpc.IndexWriter) error, error) {
	if err := func(v uint8, w io.Writer) error {
		b := make([]byte, 2)
		i := binary.PutUvarint(b, uint64(v))
		slog.Debug("writing u8 discriminant")
		_, err := w.Write(b[:i])
		return err
	}(uint8(v.discriminant), w); err != nil {
		return nil, fmt.Errorf("failed to write discriminant: %w", err)
	}
	switch v.discriminant {
	case StreamErrorDiscriminant_LastOperationFailed:
		payload, ok := v.payload.(Error)
		if !ok {
			return nil, errors.New("invalid payload")
		}
		write, err := (func(wrpc.IndexWriter) error)(nil), func(any) error { return errors.New("writing owned handles not supported yet") }(payload)
		if err != nil {
			return nil, fmt.Errorf("failed to write payload: %w", err)
		}

		if write != nil {
			return func(w wrpc.IndexWriter) error {
				w, err := w.Index(0)
				if err != nil {
					return fmt.Errorf("failed to index writer: %w", err)
				}
				return write(w)
			}, nil
		}
	case StreamErrorDiscriminant_Closed:
	default:
		return nil, errors.New("invalid variant")
	}
	return nil, nil
}

type InputStream interface {
	// Perform a non-blocking read from the stream.
	//
	// When the source of a `read` is binary data, the bytes from the source
	// are returned verbatim. When the source of a `read` is known to the
	// implementation to be text, bytes containing the UTF-8 encoding of the
	// text are returned.
	//
	// This function returns a list of bytes containing the read data,
	// when successful. The returned list will contain up to `len` bytes;
	// it may return fewer than requested, but not more. The list is
	// empty when no bytes are available for reading at this time. The
	// pollable given by `subscribe` will be ready when more bytes are
	// available.
	//
	// This function fails with a `stream-error` when the operation
	// encounters an error, giving `last-operation-failed`, or when the
	// stream is closed, giving `closed`.
	//
	// When the caller gives a `len` of 0, it represents a request to
	// read 0 bytes. If the stream is still open, this call should
	// succeed and return an empty list, or otherwise fail with `closed`.
	//
	// The `len` parameter is a `u64`, which could represent a list of u8 which
	// is not possible to allocate in wasm32, or not desirable to allocate as
	// as a return value by the callee. The callee may return a list of bytes
	// less than `len` in size while more bytes are available for reading.
	Read(ctx__ context.Context, wrpc__ wrpc.Client, len uint64) (*wrpc.Result[[]uint8, StreamError], func() error, error)
	// Read bytes from a stream, after blocking until at least one byte can
	// be read. Except for blocking, behavior is identical to `read`.
	BlockingRead(ctx__ context.Context, wrpc__ wrpc.Client, len uint64) (*wrpc.Result[[]uint8, StreamError], func() error, error)
	// Skip bytes from a stream. Returns number of bytes skipped.
	//
	// Behaves identical to `read`, except instead of returning a list
	// of bytes, returns the number of bytes consumed from the stream.
	Skip(ctx__ context.Context, wrpc__ wrpc.Client, len uint64) (*wrpc.Result[uint64, StreamError], func() error, error)
	// Skip bytes from a stream, after blocking until at least one byte
	// can be skipped. Except for blocking behavior, identical to `skip`.
	BlockingSkip(ctx__ context.Context, wrpc__ wrpc.Client, len uint64) (*wrpc.Result[uint64, StreamError], func() error, error)
	// Create a `pollable` which will resolve once either the specified stream
	// has bytes available to read or the other end of the stream has been
	// closed.
	// The created `pollable` is a child resource of the `input-stream`.
	// Implementations may trap if the `input-stream` is dropped before
	// all derived `pollable`s created with this function are dropped.
	Subscribe(ctx__ context.Context, wrpc__ wrpc.Client) (Pollable, func() error, error)
	Drop(ctx__ context.Context, wrpc__ wrpc.Client) error
}
type OutputStream interface {
	// Check readiness for writing. This function never blocks.
	//
	// Returns the number of bytes permitted for the next call to `write`,
	// or an error. Calling `write` with more bytes than this function has
	// permitted will trap.
	//
	// When this function returns 0 bytes, the `subscribe` pollable will
	// become ready when this function will report at least 1 byte, or an
	// error.
	CheckWrite(ctx__ context.Context, wrpc__ wrpc.Client) (*wrpc.Result[uint64, StreamError], func() error, error)
	// Perform a write. This function never blocks.
	//
	// When the destination of a `write` is binary data, the bytes from
	// `contents` are written verbatim. When the destination of a `write` is
	// known to the implementation to be text, the bytes of `contents` are
	// transcoded from UTF-8 into the encoding of the destination and then
	// written.
	//
	// Precondition: check-write gave permit of Ok(n) and contents has a
	// length of less than or equal to n. Otherwise, this function will trap.
	//
	// returns Err(closed) without writing if the stream has closed since
	// the last call to check-write provided a permit.
	Write(ctx__ context.Context, wrpc__ wrpc.Client, contents []uint8) (*wrpc.Result[struct{}, StreamError], func() error, error)
	// Perform a write of up to 4096 bytes, and then flush the stream. Block
	// until all of these operations are complete, or an error occurs.
	//
	// This is a convenience wrapper around the use of `check-write`,
	// `subscribe`, `write`, and `flush`, and is implemented with the
	// following pseudo-code:
	//
	// ```text
	// let pollable = this.subscribe();
	// while !contents.is_empty() {
	// // Wait for the stream to become writable
	// pollable.block();
	// let Ok(n) = this.check-write(); // eliding error handling
	// let len = min(n, contents.len());
	// let (chunk, rest) = contents.split_at(len);
	// this.write(chunk  );            // eliding error handling
	// contents = rest;
	// }
	// this.flush();
	// // Wait for completion of `flush`
	// pollable.block();
	// // Check for any errors that arose during `flush`
	// let _ = this.check-write();         // eliding error handling
	// ```
	BlockingWriteAndFlush(ctx__ context.Context, wrpc__ wrpc.Client, contents []uint8) (*wrpc.Result[struct{}, StreamError], func() error, error)
	// Request to flush buffered output. This function never blocks.
	//
	// This tells the output-stream that the caller intends any buffered
	// output to be flushed. the output which is expected to be flushed
	// is all that has been passed to `write` prior to this call.
	//
	// Upon calling this function, the `output-stream` will not accept any
	// writes (`check-write` will return `ok(0)`) until the flush has
	// completed. The `subscribe` pollable will become ready when the
	// flush has completed and the stream can accept more writes.
	Flush(ctx__ context.Context, wrpc__ wrpc.Client) (*wrpc.Result[struct{}, StreamError], func() error, error)
	// Request to flush buffered output, and block until flush completes
	// and stream is ready for writing again.
	BlockingFlush(ctx__ context.Context, wrpc__ wrpc.Client) (*wrpc.Result[struct{}, StreamError], func() error, error)
	// Create a `pollable` which will resolve once the output-stream
	// is ready for more writing, or an error has occured. When this
	// pollable is ready, `check-write` will return `ok(n)` with n>0, or an
	// error.
	//
	// If the stream is closed, this pollable is always ready immediately.
	//
	// The created `pollable` is a child resource of the `output-stream`.
	// Implementations may trap if the `output-stream` is dropped before
	// all derived `pollable`s created with this function are dropped.
	Subscribe(ctx__ context.Context, wrpc__ wrpc.Client) (Pollable, func() error, error)
	// Write zeroes to a stream.
	//
	// This should be used precisely like `write` with the exact same
	// preconditions (must use check-write first), but instead of
	// passing a list of bytes, you simply pass the number of zero-bytes
	// that should be written.
	WriteZeroes(ctx__ context.Context, wrpc__ wrpc.Client, len uint64) (*wrpc.Result[struct{}, StreamError], func() error, error)
	// Perform a write of up to 4096 zeroes, and then flush the stream.
	// Block until all of these operations are complete, or an error
	// occurs.
	//
	// This is a convenience wrapper around the use of `check-write`,
	// `subscribe`, `write-zeroes`, and `flush`, and is implemented with
	// the following pseudo-code:
	//
	// ```text
	// let pollable = this.subscribe();
	// while num_zeroes != 0 {
	// // Wait for the stream to become writable
	// pollable.block();
	// let Ok(n) = this.check-write(); // eliding error handling
	// let len = min(n, num_zeroes);
	// this.write-zeroes(len);         // eliding error handling
	// num_zeroes -= len;
	// }
	// this.flush();
	// // Wait for completion of `flush`
	// pollable.block();
	// // Check for any errors that arose during `flush`
	// let _ = this.check-write();         // eliding error handling
	// ```
	BlockingWriteZeroesAndFlush(ctx__ context.Context, wrpc__ wrpc.Client, len uint64) (*wrpc.Result[struct{}, StreamError], func() error, error)
	// Read from one stream and write to another.
	//
	// The behavior of splice is equivelant to:
	// 1. calling `check-write` on the `output-stream`
	// 2. calling `read` on the `input-stream` with the smaller of the
	// `check-write` permitted length and the `len` provided to `splice`
	// 3. calling `write` on the `output-stream` with that read data.
	//
	// Any error reported by the call to `check-write`, `read`, or
	// `write` ends the splice and reports that error.
	//
	// This function returns the number of bytes transferred; it may be less
	// than `len`.
	Splice(ctx__ context.Context, wrpc__ wrpc.Client, src InputStream, len uint64) (*wrpc.Result[uint64, StreamError], func() error, error)
	// Read from one stream and write to another, with blocking.
	//
	// This is similar to `splice`, except that it blocks until the
	// `output-stream` is ready for writing, and the `input-stream`
	// is ready for reading, before performing the `splice`.
	BlockingSplice(ctx__ context.Context, wrpc__ wrpc.Client, src InputStream, len uint64) (*wrpc.Result[uint64, StreamError], func() error, error)
	Drop(ctx__ context.Context, wrpc__ wrpc.Client) error
}
